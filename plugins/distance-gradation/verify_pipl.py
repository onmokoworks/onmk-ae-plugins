"""Verify packaged resource bytes after linking (requires Python pefile)."""
from pathlib import Path
import struct
import sys
import pefile

root = Path(__file__).resolve().parent
artifact = Path(sys.argv[1]) if len(sys.argv) > 1 else root / "dist/ONMK_DistanceGradation.aex"
pe = pefile.PE(str(artifact))
resource = next(e for e in pe.DIRECTORY_ENTRY_RESOURCE.entries if str(e.name).upper() == "PIPL")
entry = next(e for e in resource.directory.entries if e.id == 16000).directory.entries[0].data.struct
data = pe.get_data(entry.OffsetToData, entry.Size)
assert data[:6] == bytes([1, 0, 0, 0, 0, 0]), "Unexpected PiPL header"
count = struct.unpack_from("<I", data, 6)[0]
offset = 10
keys = []
for _ in range(count):
    vendor, key, ident, size = struct.unpack_from("<4s4sII", data, offset)
    assert vendor == b"MIB8", f"Corrupt property boundary at byte {offset}"
    offset += 16
    payload = data[offset:offset + size]
    assert len(payload) == size, "Truncated property"
    keys.append(key[::-1].decode("ascii"))
    offset += size
assert offset == len(data), "Unexpected bytes after PiPL properties"
candidates = list((root / "target/release/build").glob("distance-gradation-*/out/plugin.pipl"))
assert any(candidate.read_bytes() == data for candidate in candidates), "Packaged bytes differ from generated PiPL"
assert {"kind", "name", "8664", "eMNA", "eGL2"} <= set(keys)
print(f"PiPL verified: {count} properties, {len(data)} exact binary bytes")
