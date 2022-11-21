# User Authentication Tokens

Format: `goatns_<something>`

Stored in database table: `user_tokens`

Schema:

- id primary key
- issued (Not null)
- expiry (Maybe null, if it won't expire)
- userid (Foreign key users(id))
- tokenhash - String (Argon2id of the token)

How the token's generated

1. take the following strings and concatenate them with dashes in between:
   - the system cookie secret
   - the userid
   - issuance time
   - expiry (if set)
   - a random number? maybe?
2. Sha512 it
3. append "goatns_"
4. Send it to the farm

Issuance method:

1. Log into the UI
2. Go into settings, click new token
3. Select an expiry time, which is one of
   - 8h
   - 24h
   - 30d
   - Forever (null)
4. Issue it, which will calculate it, store it in the database and then show it (once) to the user.
5. Refreshing the page will reset the state of the thing and yeet you out to the settings page.
