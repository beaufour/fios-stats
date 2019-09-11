[![Build Status](https://travis-ci.org/beaufour/fios-stats.svg)](https://travis-ci.org/beaufour/fios-stats)

Simple program to fetch stats from the Fios Quantum Gateway router.

Usage
=====

    > fios-stats -p <admin password> [-i <influx_db_uri> ]

Example:

    > fios-stats -p secret_password -i 'http://192.168.0.12:8086/write?db=fios_data'

This fetches the stats using the admin password `secret_password` and stores the data in the influxdb at
`http://192.168.0.12:8086/write?db=fios_data`.

Notes
=====

This is my first Rust program so it is probably not as good Rust as it should be...

Compiling
=========

As long as you are running Rust `nightly` it should just be a question of `cargo build` and you are
good.
