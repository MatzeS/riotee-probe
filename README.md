# riotee-probe

A Rust library and CLI for controlling [Riotee](https://www.riotee.nessie-circuits.de)
debug hardware — the
[Riotee Probe](https://www.riotee.nessie-circuits.de/docs/latest/hardware/probe.html) and the
[Riotee Board](https://www.riotee.nessie-circuits.de/docs/latest/hardware/board.html).

This tool is a rust-rewrite implementation of the original [riotee_probe](https://github.com/NessieCircuits/Riotee_ProbeSoftware/tree/main/tool) python tool.
However, it is *not* a drop-in replacement. The commandline syntax is slightly changed.

The impelmentation is refactored to be used as libarary for programatic interaction with Riotee devices.


## Development Status

`riotee-probe` depends on recent patches of `probe-rs`, which have not yet landed on crates.io at this time (2026-11-25).
`riotee-probe` will be pushed to `crates.io`, once `probe-rs v0.33.0` lands, until then you may use this as git dependency.


`riotee-probe` is at an early release stage.
Major changes may be merged without major version bumb prior to `1.0.0` release.


## AI Disclosure

I developed this with assistance of claude code.
Initially, claude was instructed to reproduce the python tool functionality.
Afterwards I reviewed/refactored/reworked the implementation to enhace the library API surface and overall implementation quality.
I have verified the implementation on real hardware.
