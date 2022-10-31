# The database

What do we need to store?

Zones
    which contain

    id - u64
    name : Text
    rname : Text
    serial : Integer,
    refresh : Integer,
    retry : Integer,
    expire : Integer,
    minimum : Integer,

Records
    which contain

    id - u64
    name (String)
    ttl (Optional) - Integer (0 or NULL = Inherit from SOA)
    rtype - Integer
    rclass - Integer
    rdata (Text? Varchar?)
