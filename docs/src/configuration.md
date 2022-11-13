# Configuration

I'll totally forget to update this, so check [the rustdoc for ConfigFile](https://goatns.dotgoat.net/rustdoc/goatns/config/struct.ConfigFile.html) for up-to-date information.

To write out a "default" config file, run `goatns --export-default-config` which will dump the contents of the system defaults.


## User Authentication

This is build for [Kanidm](https://kanidm.com) but should work with any OIDC identity provider.

### User auto-provisioning

This is disabled by default, but set `user_auto_provisioning` to true and anyone who can authenticate will be able to add themselves to the system.

## Admin contact

If it's configured, this'll show up in a few places.

- the home page
- error messages
- randomly because I forgot to update the docs

These are the current supported formats:

| Contact Type | Example Format                     |
| ------------ | ---------------------------------- |
| Mastodon     | `Mastodon:yaleman@mastodon.social` |
| Email        | `Email:billy@dotgoat.net`          |
| Twitter      | `Twitter:dotgoatdomains`           |
