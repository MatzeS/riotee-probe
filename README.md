# riotee-probe

A Rust library and CLI for controlling [Riotee](https://www.riotee.nessie-circuits.de)
debug hardware — the
[Riotee Probe](https://www.riotee.nessie-circuits.de/docs/latest/hardware/probe.html) and the
[Riotee Board](https://www.riotee.nessie-circuits.de/docs/latest/hardware/board.html).

This tool is a rust-rewrite implementation of the original [riotee_probe](https://github.com/NessieCircuits/Riotee_ProbeSoftware/tree/main/tool) python tool.
However, it is *not* a drop-in replacement. The commandline syntax is slightly changed.

The impelmentation is refactored to be used as libarary for programatic interaction with Riotee devices.
