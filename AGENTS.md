# AE plugin implementation checks

For rendering or pixel-format changes, read `docs/alpha-contract.md` before implementation. AE boundary RGB is straight, not premultiplied. Specify internal association separately; do not remove alpha weighting from spatial filters blindly.

Require partial-alpha fixtures and an independent expected result. CPU/GPU agreement and encode/decode roundtrip alone are insufficient. Test that an intentionally premultiplied generator output fails the acceptance check. Record source/artifact identity, supported depths/routes and missing coverage. Do not describe source inventory or arithmetic-only tests as AE validation.

Existing known findings: `docs/alpha-audit-20261016.md`. Do not bless the current premultiplied assumptions because existing tests pass. Preserve unrelated dirty work. Do not operate AE, install or restart it as part of a test unless the user explicitly requests that action. A requested MediaCore file copy does not authorize operating AE.
