# Authenticated Unreal Game Sync backend

RUGS is an authenticated Unreal Game Sync metadata server written in Rust. It's intended
to be small, simple, usable when the endpoints might be publicly accessible on the
internet.

## Setup

1. Copy `config.json.dist` to `config.json` and put a randomly generated token into 
`ci_auth` and `user_auth`
    - `ci_auth` is used to post new badges for UGS
    - `user_auth` is used by UGS to read badges and leave user feedback
    - Both of these should be in the `user:password` format used by URLs
1. Run `./apply_migrations.sh` to initialize the database (it will be written to
`metadata.db` by default)
1. Run the server with `cargo run --release` (it will listen on port 3000)

## Use

RUGS does not (currently) support an SSL certificate. You should run it on a machine
which is not accessible directly from the internet, and configure an endpoint in front of
it which handles HTTPS -- e.g. an AWS ALB or your own nginx instance. **THIS IS 
IMPORTANT**, because the authentication is just HTTP Basic Auth, and so it'll be sent in 
plaintext over the wire if you're not using HTTPS. If you're not using HTTPS, make sure 
RUGS is only accessible from a local network (in which case you can also leave `user_auth` 
empty).

By default RUGS exposes a `/health` API which can be used to check if the service is running. It'll return an empty 200 status.

You can configure UGS by adding a section like the following to your `UnrealGameSync.ini`
after applying [this pull request](https://github.com/EpicGames/UnrealEngine/pull/9168) to your UGS which adds support for 
credentials:

```ini
[Default]
ApiUrl=https://user:password@my.rugs.local
```

Then you can run `PostBadgeStatus.exe` from `Engine/Source/Programs/UnrealGameSync` or the 
Jenkins plugin
[unreal-game-sync-badges](https://github.com/jorgenpt/unreal-game-sync-badges-plugin) to
post badges to `https://ci_auth_user:ci_auth_passwordd@my.rugs.local`.

## Caveats

Currently RUGS only support badges, it does not support reviews & build information from 
users. That'll come soon.