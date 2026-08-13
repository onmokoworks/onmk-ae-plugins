(function () {
    function quote(value) {
        return '"' + String(value)
            .replace(/\\/g, "\\\\")
            .replace(/"/g, '\\"')
            .replace(/\r/g, "\\r")
            .replace(/\n/g, "\\n") + '"';
    }

    function toJson(value) {
        var i;
        var parts;
        if (value === null || value === undefined) {
            return "null";
        }
        if (value instanceof Array) {
            parts = [];
            for (i = 0; i < value.length; i += 1) {
                parts.push(toJson(value[i]));
            }
            return "[" + parts.join(",") + "]";
        }
        if (typeof value === "object") {
            parts = [];
            for (var key in value) {
                if (value.hasOwnProperty(key)) {
                    parts.push(quote(key) + ":" + toJson(value[key]));
                }
            }
            return "{" + parts.join(",") + "}";
        }
        if (typeof value === "number" || typeof value === "boolean") {
            return String(value);
        }
        return quote(value);
    }

    function writeReport(reportPath, report) {
        var file = new File(reportPath);
        file.parent.create();
        file.encoding = "UTF-8";
        if (!file.open("w")) {
            throw new Error("Could not open smoke report: " + reportPath);
        }
        file.write(toJson(report));
        file.close();
    }

    function addEffectCheck(layer, matchName, displayName) {
        var parade = layer.property("ADBE Effect Parade");
        var effect = parade.addProperty(matchName);
        if (!effect) {
            throw new Error("Could not add effect: " + matchName);
        }
        return {
            requestedMatchName: matchName,
            expectedDisplayName: displayName,
            matchName: effect.matchName,
            name: effect.name,
            propertyCount: effect.numProperties,
            displayNameMatches: effect.name === displayName,
            propertyCountPositive: effect.numProperties > 0,
            pass: effect.matchName === matchName &&
                effect.name === displayName &&
                effect.numProperties > 0
        };
    }

    var reportPath = $.global.PARTICLELAB_AE_SMOKE_REPORT ||
        (Folder.temp.fsName + "/particlelab_ae_smoke_report.json");
    var quitAfter = $.global.PARTICLELAB_AE_SMOKE_QUIT === true;
    var smokeRunId = $.global.PARTICLELAB_AE_SMOKE_RUN_ID || "";
    var report = {
        pass: false,
        smokeRunId: smokeRunId,
        appName: app.name,
        appVersion: app.version,
        checks: [],
        errors: []
    };
    var createdProject = false;

    app.beginSuppressDialogs();
    try {
        if (!app.project) {
            app.newProject();
        }
        if (app.project.numItems > 0) {
            throw new Error("Open project is not empty. Run smoke test from a clean AE session.");
        }

        createdProject = true;
        var comp = app.project.items.addComp(
            "ParticleLabEngineCoreSmoke",
            256,
            256,
            1.0,
            1.0 / 24.0,
            24.0
        );
        var layer = comp.layers.addSolid(
            [1.0, 1.0, 1.0],
            "Smoke Solid",
            256,
            256,
            1.0,
            1.0
        );

        report.checks.push(addEffectCheck(layer, "ParticleKit", "Particle Kit"));
        report.checks.push(addEffectCheck(layer, "LatticeLab", "Lattice Lab"));
        report.pass = true;

        for (var i = 0; i < report.checks.length; i += 1) {
            if (!report.checks[i].pass) {
                report.pass = false;
            }
        }
    } catch (err) {
        report.errors.push(String(err && err.message ? err.message : err));
        report.pass = false;
    } finally {
        try {
            if (createdProject && app.project) {
                app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES);
            }
        } catch (closeErr) {
            report.errors.push("close failed: " + String(closeErr));
            report.pass = false;
        }
        try {
            writeReport(reportPath, report);
        } catch (writeErr) {
            alert("ParticleLab AE smoke report failed: " + writeErr);
        }
        app.endSuppressDialogs(false);
        if (quitAfter && createdProject) {
            app.quit();
        }
    }
})();
