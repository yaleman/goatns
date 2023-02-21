# The database

What do we need to store?

## Zones

Which contain:

- id - u64
- name : Text
- rname : Text
- serial : Integer,
- refresh : Integer,
- retry : Integer,
- expire : Integer,
- minimum : Integer,

## Records

Which contain

- id - u64
- name (String)
- ttl (Optional) - Integer (0 or NULL = Inherit from SOA)
- rtype - Integer
- rclass - Integer
- rdata (Text? Varchar?)

## Zones for a user

There's two main things:

If the user's an admin, they see everything.

```sql
SELECT id, name FROM zones ORDER BY NAME OFFSET ? LIMIT ?
```

Otherwise, it's slightly more complex..

```sql
SELECT zones.id as zoneid, name 
FROM zones, ownership 
WHERE zones.id = ownership.zoneid AND ownership.userid = 1
LIMIT ? OFFSET ?;
```
