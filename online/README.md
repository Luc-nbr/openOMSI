# Omsi-Hub online backend

Server-side code for Omsi-Hub's optional online features. Everything runs in the Neon project
`omsi-hub-online` (`frosty-water-75529121`), branch `main` (`br-shy-water-b9vx3gyb`), region
`aws-eu-central-1`. The design and the security model are in
[`docs/omsihub/ONLINE.md`](../docs/omsihub/ONLINE.md).

| Path | What |
|---|---|
| `functions/saves/index.mjs` | the `saves` Neon Function: cloud saves (presigned S3 URLs, no dependencies) |
| `functions/saves/test.mjs` | its unit tests |
| `functions/board/index.mjs` | the `board` Neon Function: the public, opt-in leaderboard for the website (`GET /?limit=1-100`, CORS open, cached 30 s, no dependencies) |
| `functions/board/test.mjs` | its unit tests |
| `leaderboard.sql` | the leaderboard's columns and functions (`set_leaderboard`, `report_stats`, `hub_leaderboard`, `hub_board_counts`), applied 2026-10-16 |

No secret belongs in this folder. The function gets everything it needs from the variables Neon
injects at runtime: `NEON_AUTH_JWKS_URL`, `NEON_AUTH_BASE_URL`, `AWS_ACCESS_KEY_ID`,
`AWS_SECRET_ACCESS_KEY`, `AWS_ENDPOINT_URL_S3` and `AWS_REGION`.

## Test

```sh
cd online/functions/saves
node --test
```

Node 24, no `npm install`. The tests cover AWS's published SigV4 examples (a presigned GET, a
signed GET object and a signed list), the JWT checks with real Ed25519 keys, the input validation,
and every route against an in-memory bucket.

## Deploy

The function has no dependencies, so the deploy zip holds only `index.mjs` at its root (no
bundling needed). Any one of these works.

**Neon CLI** (`npm i -g neon@latest`, then `neon link` to the project and branch above):

```sh
neon functions deploy saves --src online/functions/saves/index.mjs --no-bundle --branch br-shy-water-b9vx3gyb
```

**Neon API:**

```powershell
Compress-Archive -Path online\functions\saves\index.mjs -DestinationPath $env:TEMP\saves.zip -Force
curl.exe -X POST "https://console.neon.tech/api/v2/projects/frosty-water-75529121/branches/br-shy-water-b9vx3gyb/functions/saves/deployments" `
  -H "Authorization: Bearer $env:NEON_API_KEY" -F "zip=@$env:TEMP\saves.zip" -F "runtime=nodejs24"
```

**Neon MCP server:** zip as above, base64 the zip, and call `deploy_function` with slug `saves`,
runtime `nodejs24` and the branch id.

Then check it:

1. `neon functions get saves` (or the MCP `get_function`) until the deployment is `completed`.
2. Smoke test without an account. All of these must answer `401 {"ok":false,"error":"unauthorized"}`:

   ```sh
   U=https://br-shy-water-b9vx3gyb-saves.compute.c-7.eu-central-1.aws.neon.tech
   curl -i -X POST $U/list -d '{}'                                        # no token
   curl -i -X POST $U/list -H 'authorization: Bearer garbage.token.x' -d '{}'
   curl -i -X POST $U/no_such_route -d '{}'                               # unknown route, no token
   ```

3. Read the function logs (Console › Monitoring › Logs, or the MCP `query_logs` with source
   `function`). Each new instance logs one line,
   `{"fn":"saves","event":"startup","jwks_keys":1,"storage":"ok"}`: the JWKS loaded, and a
   read-only signed HEAD of a missing key answered 404, so the SigV4 signing and the storage
   credentials work. Any other `storage` value means the S3 side is broken.

The slug `saves` is permanent and is part of the URL, so a redeploy keeps the URL.

## The `board` function

Deployed like `saves` (slug `board`, runtime `nodejs24`, only `index.mjs` in the zip). It reads the
database through Neon's SQL-over-HTTP endpoint with the `DATABASE_URL` Neon injects, and only ever
calls `hub_leaderboard` and `hub_board_counts`. URL:
`https://br-shy-water-b9vx3gyb-board.compute.c-7.eu-central-1.aws.neon.tech/`. Smoke test: `GET /`
answers `200 {"ok":true,...}` with `access-control-allow-origin: *`; `POST /` answers 405.
