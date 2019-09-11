Simple program to fetch stats from the Fios Quantum Gateway router.

Usage:

    > fios-stats -p <admin password> [-i <influx_db_uri> ]

Example:

    > fios-stats -p secret_password -i 'http://192.168.0.12:8086/write?db=fios_data'

This fetches the stats using the admin password `secret_password` and stores the data in the influxdb at
`http://192.168.0.12:8086/write?db=fios_data`.
