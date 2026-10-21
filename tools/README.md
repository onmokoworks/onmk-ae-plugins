# Tools

Repository-wide build, verification, migration, and deployment tools live here.


Alpha checks: see [contract](../docs/alpha-contract.md). `alpha_contract.py` checks packed raw RGBA fixtures; `test_alpha_contract.py` includes premultiplication negative controls. `alpha_source_probes.py` audits two exact production functions and currently exits 1 for known defects. These are not substitutes for native per-plugin validation.
