# The database

What do we need to store?

Zones
    which contain
    ZoneID - u64
    Name (Text)

    SOA Data?
        Rname : "billy@hello.goat",
        Serial : 1,
        Refresh : 2,
        Retry : 3,
        Expire : 4,
        Minimum : 60,

Records
    which contain
    RecordID - u64
    RecordType - Integer
    RecordClass - Integer
    TTL (Optional) - Integer (0 = None, Inherit from SOA)
    Name (String)
    RecordData (Text? Varchar?)

Butts.
