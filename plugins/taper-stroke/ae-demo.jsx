// Run in AE after installing ONMK_TaperStroke.aex. Adds one comp; preserves the project.
(function () {
    app.beginUndoGroup("TaperStroke demo");
    try {
        var comp = app.project.items.addComp("TaperStroke demo", 800, 500, 1, 3, 24);
        for (var n = 0; n < 3; n++) {
            var layer = comp.layers.addSolid([0, 0, 0], ["Pointed", "Round end", "Flat cap"][n], 800, 500, 1, 3);
            var mask = layer.property("ADBE Mask Parade").addProperty("ADBE Mask Atom");
            mask.maskMode = MaskMode.NONE;
            var shape = new Shape();
            shape.closed = false;
            shape.vertices = [[65, 90 + n * 150], [735, 90 + n * 150]];
            shape.inTangents = [[0, 0], [-245, 110]];
            shape.outTangents = [[185, -100], [0, 0]];
            mask.property("ADBE Mask Shape").setValueAtTime(0, shape);
            shape.outTangents = [[185, 100], [0, 0]];
            shape.inTangents = [[0, 0], [-245, -110]];
            mask.property("ADBE Mask Shape").setValueAtTime(2, shape);
            var fx = layer.property("ADBE Effect Parade").addProperty("ONMK_TaperStroke");
            fx.property(1).setValue(1);
            fx.property(2).setValueAtTime(0, 48);
            fx.property(2).setValueAtTime(2, 24);
            fx.property(3).setValue(20);
            fx.property(4).setValue(n === 1 ? 35 : 0);
            fx.property(5).setValue(n === 2 ? 2 : 1);
            fx.property(6).setValue(n === 2 ? 2 : 1);
            fx.property(7).setValue([0.94, 0.73, 0.37]);
            fx.property(8).setValue(100);
        }
        comp.openInViewer();
    } finally {
        app.endUndoGroup();
    }
}());
