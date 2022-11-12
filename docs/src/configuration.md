# Configuration

I'll totally forget to update this, so check [the rustdoc for ConfigFile](https://goatns.dotgoat.net/rustdoc/goatns/config/struct.ConfigFile.html) for up-to-date information.

## User Authentication

This is build for [Kanidm](https://kanidm.com) but should work with any OIDC identity provider.

### User auto-provisioning

This is disabled by default, but set `user_auto_provisioning` to true and anyone who can authenticate will be able to add themselves to the system.