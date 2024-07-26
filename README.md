# Authenticated Unreal Game Sync backend

[![Latest Docker image][badge-latest]][container-registry]
[![Docker image size][badge-size]][container-registry]

RUGS is an efficient, easy-to-deploy alternative to Epic's official metadata
server for [Unreal Game Sync][ugs]. It is designed to work seamlessly with
Unreal Game Sync, and adds support for basic authentication if desired. It uses
a simple sqlite database to have as little setup as possible.

The basic authentication is intended to allow you to run it publicly accessible
on the internet with a modicum of security, and requires a [small
patch][ugs-pull] applied to Unreal Game Sync before it's available.

Features that a RUGS server will enable in Unreal Game Sync:

- Surfacing build results and providing desktop notifications for build
  breakages
- Allowing users to mark changes as good and bad, and indicate to other team
  members that they're investigating a problem with the build
- Showing which users are synced to which changes

## Setup with Docker (recommended)

Docker images are available for arm64 and amd64, and are [hosted on the GitHub
Container Registry][container-registry]. The `latest` tag is used for official
releases, or there is a `main` tag for bleeding edge builds.

Example command to run the image:

```sh
docker run \
  -e RUGS_USER_AUTH=ugs:super_secret_password \
  -e RUGS_CI_AUTH=ci:even_super_secreter_password \
  -p 3000:3000 \
  --mount "type=volume,source=rugs_data,destination=/data" \
  --name rugs \
  ghcr.io/jorgenpt/rugs:latest
```

You can read about what the environment variables do in [the section
below](#environment-variables).

RUGS expects `/data` to be **persistent between sessions**. The recommended
approach is to [mount a Docker volume](https://docs.docker.com/storage/volumes/)
at that location. You can change the HTTP port by using `-p <desired
port>:3000` instead of `-p 3000:3000`.

RUGS will automatically create and migrate the database on startup, so when you
upgrade, there should not be any other steps needed.

You can also look at [examples/docker-compose.yml](/examples/docker-compose.yml)
for a complete setup that supports HTTPS by automatically requesting a
certificate from Let's Encrypt.

## Setup locally

1. Run `./apply_migrations.sh` to initialize the database (it will be written to
   `metadata.db` by default)
1. Run the server by setting the [appropriate environment
   variables](#environment-variables) and then running `cargo run --release`

## Configure Unreal Game Sync

### Authenticated (e.g. publicly on the internet)

You can configure UGS by adding a section like the following to your
`UnrealGameSync.ini` after applying [this pull request][ugs-pull] to your UGS to
adds support for HTTP basic auth credentials:

```ini
[Default]
ApiUrl=https://ugs:super_secret_password@my.rugs.local
```

See also [the note about HTTPS](#https)

### Unauthenticated (only for private networks)

You can configure UGS by adding a section like the following to your
`UnrealGameSync.ini`, and you can use Unreal Game Sync out of the box with no
changes:

```ini
[Default]
ApiUrl=http://my.rugs.local
```

## Submit badges from CI

To submit badges, you can do one of the following:

- Run `PostBadgeStatus.exe` from `Engine/Source/Programs/UnrealGameSync` (needs
  [pull request #9168][ugs-pull] to support authentication), or
- Use the Teamcity plugin
  [teamcity-ugs-status-publisher](https://github.com/jorgenpt/teamcity-ugs-status-publisher)
  to post badges to `https://ci_auth_user:ci_auth_password@my.rugs.local`, or
- Use the Jenkins plugin
  [unreal-game-sync-badges](https://github.com/jorgenpt/unreal-game-sync-badges-plugin)
  to post badges to `https://ci_auth_user:ci_auth_password@my.rugs.local`, or
- Make a direct request to the API -- see [submitting badges](#submitting-badges).

## Additional setup information

### APIs

By default RUGS exposes a `/health` API which can be used to check if the
service is running. It'll return an empty 200 status.

### HTTPS

RUGS does not (currently) support an SSL certificate. You should run it on a
machine which is not accessible directly from the internet, and configure an
endpoint in front of it which handles HTTPS -- e.g. an AWS ALB or your own nginx
instance.

**THIS IS IMPORTANT**, because the authentication is just HTTP Basic
Auth, and so it'll be sent in plaintext over the wire if you're not using HTTPS.
If you're not using HTTPS, make sure RUGS is only accessible from a local
network (in which case you can also leave `RUGS_USER_AUTH` empty).

You can also look at [examples/docker-compose.yml](/examples/docker-compose.yml)
for a complete setup that supports HTTPS by automatically requesting a
certificate from Let's Encrypt.

### Environment variables

- `RUGS_USER_AUTH`: Username and password used for basic auth used by Unreal
  Game Sync, in `user:pass` format. Defaults to empty, allow anyone to query
  this without authentication.
- `RUGS_CI_AUTH`: Username and password used for basic auth used to submit
  badges to RUGS (via CI plugins, PostBadgeStatus.exe, etc), in `user:pass`
  format. Defaults to empty, allowing anyone to use this API without
  authentication.
- `RUGS_WEB_ROOT`: The prefix to all the paths we listen to. Defaults to `/`.
- `RUGS_PORT`: The HTTP port we listen on. Defaults to 3000. Rarely used with
  docker, as you can just use `-p <desired port>:3000`

### Submitting badges

If you want more control over submitting badges from CI, you can make a `POST`
request to `/api/build`. You need to use HTTP Basic Auth with the `RUGS_CI_AUTH`
(if any), and provide a JSON body like:

```json
{
  "Project": "//myproject/main/MyProject",
  "ChangeNumber": 123,
  "BuildType": "Editor",
  "Result": "Starting",
  "Url": "https://my.ci/jobs/100"
}
```

These fields are:

- `Project`: The Perforce depot path to the project directory, i.e. the
  directory where the `.uproject` file lives (so `//myproject/main/MyProject`,
  not `//myproject/main` or `//myproject/main/MyProject/MyProject.uproject`)
- `ChangeNumber`: The Perforce changelist number that the badge is associated
  with
- `BuildType`: Arbitrary identifier used to update the status of the same badge
  (a new request with the same `BuildType` and `ChangeNumber` will overwrite an
  old badge)
- `Result`: The status color shown in UGS, which can be one of `Starting`,
  `Failure`, `Warning`, `Success`, or `Skipped`
- `Url`: The address that will be opened when the badge is clicked in UGS

### Docker volume backup

To back the data from your Docker volume up, you can use the following command
to create a `backup.tar` in the current directory:

```sh
docker run --rm \
  --volumes-from rugs \
  --mount "type=bind,src=$(pwd),dst=/backup" \
  debian:stable-slim \
  tar cvf "/backup/rugs-backup.$(date +"%Y%m%d.%H%M%S").tar" /data
```

From within that same directory, you can restore the data with the following command:

```sh
docker run --rm \
  --volumes-from rugs \
  --mount "type=bind,src=$(pwd),dst=/backup" \
  debian:stable-slim \
  tar -xvf /backup/rugs-backup.<timestamp>.tar -C /data
```

Where `<timestamp>` is the timestamp of the backup you want to restore.

This assumes that the container name is `rugs` on your Docker container.

## License

This work is dual-licensed under Apache 2.0 and MIT.
You can choose between one of them if you use this work.

`SPDX-License-Identifier: MIT OR Apache-2.0`

[ugs]: https://docs.unrealengine.com/5.2/en-US/unreal-game-sync-ugs-for-unreal-engine/
[ugs-pull]: https://github.com/EpicGames/UnrealEngine/pull/9168
[container-registry]: https://github.com/jorgenpt/rugs/pkgs/container/rugs
[badge-latest]: https://ghcr-badge.egpl.dev/jorgenpt/rugs/latest_tag?trim=major&label=latest&ignore=latest,main,docker
[badge-size]: https://ghcr-badge.egpl.dev/jorgenpt/rugs/size?trim=major&ignore=latest,main,docker
