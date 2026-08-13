$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$ShellRoot = Join-Path $Root "tools\node-ui-shell"

function Invoke-Step {
    param(
        [string]$Name,
        [scriptblock]$Script
    )

    Write-Host "==> $Name"
    & $Script
}

function Assert-FileExists {
    param([string]$RelativePath)

    $path = Join-Path $Root $RelativePath
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Missing expected file: $RelativePath"
    }
}

function Read-Text {
    param([string]$RelativePath)
    Get-Content -Raw -LiteralPath (Join-Path $Root $RelativePath)
}

function Assert-Contains {
    param(
        [string]$Text,
        [string]$Needle,
        [string]$Label
    )

    if (-not $Text.Contains($Needle)) {
        throw "$Label does not contain expected text: $Needle"
    }
}

function Get-Json {
    param([string]$RelativePath)

    Get-Content -Raw -LiteralPath (Join-Path $Root $RelativePath) | ConvertFrom-Json
}

function As-Array {
    param($Value)

    if ($null -eq $Value) {
        return @()
    }
    return @($Value)
}

function Test-HasProperty {
    param(
        $Object,
        [string]$Name
    )

    if ($null -eq $Object) {
        return $false
    }
    return $null -ne $Object.PSObject.Properties[$Name]
}

function Get-Property {
    param(
        $Object,
        [string]$Name
    )

    if ($null -eq $Object) {
        return $null
    }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }
    return $property.Value
}

function Test-JsonNumber {
    param($Value)

    return (
        $Value -is [byte] -or
        $Value -is [sbyte] -or
        $Value -is [int16] -or
        $Value -is [uint16] -or
        $Value -is [int32] -or
        $Value -is [uint32] -or
        $Value -is [int64] -or
        $Value -is [uint64] -or
        $Value -is [single] -or
        $Value -is [double] -or
        $Value -is [decimal]
    )
}

function Test-JsonInteger {
    param($Value)

    if (-not (Test-JsonNumber $Value)) {
        return $false
    }
    return ([double]$Value) -eq [math]::Floor([double]$Value)
}

function Test-SocketValue {
    param(
        $Value,
        [string]$ValueType,
        $EnumOptions
    )

    switch ($ValueType) {
        "boolean" { return $Value -is [bool] }
        "float" { return Test-JsonNumber $Value }
        "integer" { return Test-JsonInteger $Value }
        "vector3" {
            $items = As-Array $Value
            return $items.Count -eq 3 -and @($items | Where-Object { -not (Test-JsonNumber $_) }).Count -eq 0
        }
        "color" {
            $items = As-Array $Value
            return $items.Count -eq 4 -and @($items | Where-Object { -not (Test-JsonNumber $_) }).Count -eq 0
        }
        "enum" {
            if (-not ($Value -is [string])) {
                return $false
            }
            foreach ($option in (As-Array $EnumOptions)) {
                if ($option.value -eq $Value) {
                    return $true
                }
            }
            return $false
        }
        default { return $true }
    }
}

function Test-PublishedValue {
    param(
        $Value,
        [string]$ValueType,
        $EnumOptions
    )

    if ($null -eq $Value -or $Value.type -ne $ValueType) {
        return $false
    }
    return Test-SocketValue $Value.value $ValueType $EnumOptions
}

function Find-CatalogEntry {
    param(
        [hashtable]$CatalogByType,
        [string]$NodeType
    )

    if ($CatalogByType.ContainsKey($NodeType)) {
        return $CatalogByType[$NodeType]
    }
    return $null
}

function Test-ConnectionSocket {
    param(
        $Sockets,
        [string]$SocketName
    )

    foreach ($socket in (As-Array $Sockets)) {
        if ($socket.socket -eq $SocketName) {
            return $true
        }
    }
    return $false
}

function Test-BootstrapFixture {
    param(
        [string]$RelativePath,
        [string]$ExpectedProductId,
        [int]$ExpectedSnapshotVersion
    )

    $payload = Get-Json $RelativePath
    if ($payload.version -ne 1) {
        throw "$RelativePath has unexpected bootstrap version $($payload.version)."
    }
    if ($payload.catalog.product_id -ne $ExpectedProductId) {
        throw "$RelativePath has product_id $($payload.catalog.product_id), expected $ExpectedProductId."
    }
    if ($payload.catalog.catalog_version -ne 1 -or $payload.catalog.graph_schema_version -ne 1) {
        throw "$RelativePath must use catalog_version=1 and graph_schema_version=1."
    }
    if ($payload.state.version -ne $ExpectedSnapshotVersion) {
        throw "$RelativePath has state version $($payload.state.version), expected $ExpectedSnapshotVersion."
    }
    if ($payload.state.document.schema_version -ne 1) {
        throw "$RelativePath must use graph document schema_version=1."
    }

    $namespace = if ($ExpectedProductId -eq "latticelab") { "lattice." } else { "particle." }
    $catalogByType = @{}
    foreach ($entry in (As-Array $payload.catalog.nodes)) {
        if (-not ($entry.node_type -is [string]) -or -not $entry.node_type.StartsWith($namespace)) {
            throw "$RelativePath catalog contains non-product node type $($entry.node_type)."
        }
        if ($catalogByType.ContainsKey($entry.node_type)) {
            throw "$RelativePath catalog has duplicate node type $($entry.node_type)."
        }
        $catalogByType[$entry.node_type] = $entry
    }
    if ($catalogByType.Count -eq 0) {
        throw "$RelativePath catalog is empty."
    }

    $nodesById = @{}
    foreach ($node in (As-Array $payload.state.document.nodes)) {
        $id = [string]$node.id
        if ($nodesById.ContainsKey($id)) {
            throw "$RelativePath document has duplicate node id $id."
        }
        if (-not ($node.type -is [string]) -or -not $node.type.StartsWith($namespace)) {
            throw "$RelativePath document contains non-product node type $($node.type)."
        }
        $entry = Find-CatalogEntry $catalogByType $node.type
        if ($null -eq $entry) {
            throw "$RelativePath document node $id references unknown catalog type $($node.type)."
        }
        $data = $node.data
        foreach ($socket in (As-Array $entry.value_sockets)) {
            if (-not (Test-HasProperty $data $socket.socket)) {
                throw "$RelativePath node $id is missing value socket $($socket.socket)."
            }
            $value = Get-Property $data $socket.socket
            if (-not (Test-SocketValue $value $socket.value_type $socket.enum_options)) {
                throw "$RelativePath node $id value $($socket.socket) is not $($socket.value_type)."
            }
        }
        $nodesById[$id] = $node
    }

    if (-not $nodesById.ContainsKey([string]$payload.state.document.output_node)) {
        throw "$RelativePath output_node $($payload.state.document.output_node) does not exist."
    }

    foreach ($edge in (As-Array $payload.state.document.edges)) {
        $fromId = [string]$edge.from.node
        $toId = [string]$edge.to.node
        if (-not $nodesById.ContainsKey($fromId)) {
            throw "$RelativePath edge source $fromId does not exist."
        }
        if (-not $nodesById.ContainsKey($toId)) {
            throw "$RelativePath edge target $toId does not exist."
        }
        $sourceEntry = Find-CatalogEntry $catalogByType $nodesById[$fromId].type
        $targetEntry = Find-CatalogEntry $catalogByType $nodesById[$toId].type
        if (-not (Test-ConnectionSocket $sourceEntry.output_sockets $edge.from.socket)) {
            throw "$RelativePath edge source $fromId has no output socket $($edge.from.socket)."
        }
        if (-not (Test-ConnectionSocket $targetEntry.input_sockets $edge.to.socket)) {
            throw "$RelativePath edge target $toId has no input socket $($edge.to.socket)."
        }
    }

    $publishedById = @{}
    foreach ($published in (As-Array $payload.state.document.published_params)) {
        if ([string]::IsNullOrWhiteSpace($published.stable_id)) {
            throw "$RelativePath has published param with empty stable_id."
        }
        if ($publishedById.ContainsKey($published.stable_id)) {
            throw "$RelativePath has duplicate published param $($published.stable_id)."
        }
        $targetId = [string]$published.target.node
        if (-not $nodesById.ContainsKey($targetId)) {
            throw "$RelativePath published target $targetId does not exist."
        }
        $targetEntry = Find-CatalogEntry $catalogByType $nodesById[$targetId].type
        $targetSocket = $null
        foreach ($socket in (As-Array $targetEntry.value_sockets)) {
            if ($socket.socket -eq $published.target.socket) {
                $targetSocket = $socket
                break
            }
        }
        if ($null -eq $targetSocket) {
            throw "$RelativePath published target socket $($published.target.socket) does not exist."
        }
        if ($targetSocket.value_type -ne $published.value_type) {
            throw "$RelativePath published $($published.stable_id) does not match target type."
        }
        if (-not (Test-PublishedValue $published.default_value $published.value_type $targetSocket.enum_options)) {
            throw "$RelativePath published $($published.stable_id) default value is invalid."
        }
        $publishedById[$published.stable_id] = $published
    }

    foreach ($override in (As-Array $payload.state.published_values)) {
        if (-not $publishedById.ContainsKey($override.stable_id)) {
            throw "$RelativePath published override $($override.stable_id) has no published param."
        }
        $published = $publishedById[$override.stable_id]
        if (-not (Test-PublishedValue $override.value $published.value_type @())) {
            throw "$RelativePath published override $($override.stable_id) has invalid value."
        }
    }

    $bindingSlots = @{}
    $bindingIds = @{}
    foreach ($binding in (As-Array $payload.state.host_float_bindings)) {
        if (-not (Test-JsonInteger $binding.slot) -or $binding.slot -lt 1 -or $binding.slot -gt 4) {
            throw "$RelativePath host float binding $($binding.stable_id) has invalid slot $($binding.slot)."
        }
        if ($bindingSlots.ContainsKey([string]$binding.slot)) {
            throw "$RelativePath has duplicate host float slot $($binding.slot)."
        }
        if ($bindingIds.ContainsKey($binding.stable_id)) {
            throw "$RelativePath has duplicate host float binding $($binding.stable_id)."
        }
        if (-not $publishedById.ContainsKey($binding.stable_id)) {
            throw "$RelativePath host float binding $($binding.stable_id) has no published param."
        }
        if ($publishedById[$binding.stable_id].value_type -ne "float") {
            throw "$RelativePath host float binding $($binding.stable_id) is not a float published param."
        }
        $bindingSlots[[string]$binding.slot] = $true
        $bindingIds[$binding.stable_id] = $true
    }
}

Invoke-Step "Check Node UI shell files" {
    Assert-FileExists "tools\node-ui-shell\index.html"
    Assert-FileExists "tools\node-ui-shell\styles.css"
    Assert-FileExists "tools\node-ui-shell\app.js"
    Assert-FileExists "tools\node-ui-shell\startup-payload.js"
    Assert-FileExists "tools\node-ui-shell\fixtures\particlelab-bootstrap.json"
    Assert-FileExists "tools\node-ui-shell\fixtures\latticelab-bootstrap.json"
}

Invoke-Step "Check static HTML bindings" {
    $index = Read-Text "tools\node-ui-shell\index.html"
    $app = Read-Text "tools\node-ui-shell\app.js"

    Assert-Contains $index 'href="./styles.css"' "tools/node-ui-shell/index.html"
    Assert-Contains $index 'src="./startup-payload.js"' "tools/node-ui-shell/index.html"
    Assert-Contains $index 'src="./app.js" defer' "tools/node-ui-shell/index.html"

    $htmlIds = @{}
    foreach ($match in [regex]::Matches($index, '\bid="([^"]+)"')) {
        $htmlIds[$match.Groups[1].Value] = $true
    }
    foreach ($match in [regex]::Matches($app, 'document\.getElementById\("([^"]+)"\)')) {
        $id = $match.Groups[1].Value
        if (-not $htmlIds.ContainsKey($id)) {
            throw "app.js references missing index.html id: $id"
        }
    }
}

Invoke-Step "Check JavaScript syntax" {
    & node "--check" (Join-Path $ShellRoot "app.js")
    if ($LASTEXITCODE -ne 0) {
        throw "node --check failed for tools/node-ui-shell/app.js."
    }
}

Invoke-Step "Check startup payload bridge" {
    $startup = Read-Text "tools\node-ui-shell\startup-payload.js"
    Assert-Contains $startup "window.PARTICLELAB_NODE_UI_BOOTSTRAP = null;" "tools/node-ui-shell/startup-payload.js"
    Assert-Contains $startup 'window.PARTICLELAB_NODE_UI_BOOTSTRAP_SOURCE = "";' "tools/node-ui-shell/startup-payload.js"
    $app = Read-Text "tools\node-ui-shell\app.js"
    Assert-Contains $app "PARTICLELAB_NODE_UI_BOOTSTRAP" "tools/node-ui-shell/app.js"
    Assert-Contains $app "PARTICLELAB_NODE_UI_BOOTSTRAP_SOURCE" "tools/node-ui-shell/app.js"
}

Invoke-Step "Check Node UI fixture integrity" {
    Test-BootstrapFixture "tools\node-ui-shell\fixtures\particlelab-bootstrap.json" "particlelab" 2
    Test-BootstrapFixture "tools\node-ui-shell\fixtures\latticelab-bootstrap.json" "latticelab" 1
}

Write-Host "Node UI shell verification passed."
