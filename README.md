# Authenticated Unreal Game Sync backend

RUGS is an authenticated Unreal Game Sync metadata server written in Rust. It's
intended to be small, simple, usable when the endpoints might be publicly
accessible on the internet.

## Setup

1. Run `./apply_migrations.sh` to initialize the database (it will be written to
   `metadata.db` by default)
1. Run the server by setting the appropriate environment variables and then running `cargo run --release`:
   - `RUGS_USER_AUTH`: Username and password used for basic auth used by Unreal
     Game Sync, in `user:pass` format. Defaults to empty, allow anyone to query
     this without authentication.
   - `RUGS_CI_AUTH`: Username and password used for basic auth used to submit
     badges to RUGS (via CI plugins, PostBadgeStatus.exe, etc), in `user:pass`
     format. Defaults to empty, allowing anyone to use this API without
     authentication.
   - `RUGS_PORT`: The HTTP port we listen on. Defaults to 3000.
   - `RUGS_WEB_ROOT`: The prefix to all the paths we listen to. Defaults to `/`.

## Use

RUGS does not (currently) support an SSL certificate. You should run it on a
machine which is not accessible directly from the internet, and configure an
endpoint in front of it which handles HTTPS -- e.g. an AWS ALB or your own nginx
instance. **THIS IS IMPORTANT**, because the authentication is just HTTP Basic
Auth, and so it'll be sent in plaintext over the wire if you're not using HTTPS.
If you're not using HTTPS, make sure RUGS is only accessible from a local
network (in which case you can also leave `RUGS_USER_AUTH` empty).

By default RUGS exposes a `/health` API which can be used to check if the
service is running. It'll return an empty 200 status.

You can configure UGS by adding a section like the following to your
`UnrealGameSync.ini` after applying [this pull
request](https://github.com/EpicGames/UnrealEngine/pull/9168) to your UGS which
adds support for credentials:

```ini
[Default]
ApiUrl=https://user:password@my.rugs.local
```

To submit badges, you can do one of the following:

- Run `PostBadgeStatus.exe` from `Engine/Source/Programs/UnrealGameSync`, or
- Use the Jenkins plugin
  [unreal-game-sync-badges](https://github.com/jorgenpt/unreal-game-sync-badges-plugin)
  to post badges to `https://ci_auth_user:ci_auth_password@my.rugs.local`, or
- Use the Teamcity plugin
  [teamcity-ugs-status-publisher](https://github.com/jorgenpt/teamcity-ugs-status-publisher)
  to post badges to `https://ci_auth_user:ci_auth_password@my.rugs.local`

## License

This work is dual-licensed under Apache 2.0 and MIT.
You can choose between one of them if you use this work.

`SPDX-License-Identifier: MIT OR Apache-2.0`