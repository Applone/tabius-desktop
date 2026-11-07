# Configuration sync and encryption

The desktop client performs encryption and merging locally. A lightweight server
only needs to authenticate requests and store configuration content as an opaque
UTF-8 string. The HTTP API retains the Tabby paths; modern encryption does not
require Tabby-Web or server-side cryptography.

## HTTP contract

All requests use HTTPS, `Authorization: Bearer <token>`, and
`Accept: application/json`. Redirects are rejected. A server URL can include a
base path, which prefixes the following routes.

| Request                     | Response or behavior                                                                                                                                                            |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GET /api/1/configs`        | JSON array of objects containing positive integer `id` and string `name`. IDs must fit JavaScript's safe integer range.                                                         |
| `GET /api/1/configs/{id}`   | JSON object containing the requested `id`, a string `content`, and optionally `modified_at`. Return a strong `ETag` for conditional writes.                                     |
| `PATCH /api/1/configs/{id}` | JSON body `{"content":"<YAML string>"}`. Atomically compare `If-Match` with the current ETag before writing. Return a JSON success body or an empty response, such as HTTP 204. |

The client reads the configuration again immediately before PATCH, comparing
`content` and `modified_at`. It sends the first read's ETag as `If-Match` when
available. The server should return HTTP 412 or 409 for a stale write, and 401 or
403 for invalid authorization. The second read alone does not close the race
between reading and writing; implement atomic conditional writes for that.

Configuration content is limited to 8 MiB, including YAML and base64 overhead.
JSON responses are limited to 12 MiB. Every nonempty response must be valid JSON.
Configuration creation and token provisioning are server responsibilities; the
desktop client lists and updates existing configurations.

## Modern vault, version 2

The YAML document has `encrypted: true` and a `vault` mapping. The only
unencrypted fields are this marker and the vault envelope. The decrypted vault
is a UTF-8 JSON object whose `config` member holds the full configuration and
whose `secrets` member retains the existing Tabby secret structure. Unrelated
vault members are preserved. Imported legacy secrets-only configurations are
wrapped in whole-configuration encryption, preserving their secrets.

The envelope has exactly these fields:

| Field           | Required value                                                                      |
| --------------- | ----------------------------------------------------------------------------------- |
| `version`       | Integer `2`                                                                         |
| `cipher`        | `"xchacha20-poly1305"`                                                              |
| `kdf`           | `{"name":"argon2id","version":19,"memoryKiB":65536,"iterations":3,"parallelism":4}` |
| `keyDerivation` | `"hkdf-sha256"`                                                                     |
| `keySalt`       | Hex encoding of 16 cryptographically random bytes                                   |
| `nonce`         | Hex encoding of 24 cryptographically random bytes                                   |
| `contents`      | Standard base64 encoding of ciphertext followed by its 16-byte Poly1305 tag         |

Generate a fresh salt and nonce on every encryption. Writers use lowercase hex
and padded base64; readers also accept uppercase hex and unpadded standard
base64. Empty passwords are rejected. Password bytes are UTF-8 without trimming
or Unicode normalization.

Derive the encryption key in two steps:

1. Argon2id version 19, memory 65536 KiB, time cost 3, parallelism 4, output
   length 32 bytes. Use the password bytes and decoded `keySalt`, with no Argon2
   secret or associated data. These parameters use the memory-constrained
   profile from [RFC 9106](https://datatracker.ietf.org/doc/html/rfc9106#section-4).
2. HKDF-SHA256 Extract-and-Expand using that 32-byte output as input key material,
   no salt (equivalent to 32 zero bytes), the ASCII info string
   `tabius/vault/v2/encryption-key`, and a 32-byte output key.

Encrypt the JSON bytes using XChaCha20-Poly1305 with this key and decoded nonce.
The associated data is the following fixed ASCII byte sequence followed by the
16 raw salt bytes and 24 raw nonce bytes, with no separators after the prefix:

```text
tabius-vault-v2\0argon2id-v19\0m=65536,t=3,p=4\0hkdf-sha256\0xchacha20-poly1305\0
```

Here each `\0` denotes one NUL byte. There is no trailing newline. This binary
encoding avoids any dependency on JSON field order or YAML serialization for
authentication. Readers reject unknown envelope fields, versions, algorithms,
or KDF parameters before key derivation. Remote values never determine KDF
memory or CPU cost. A future parameter profile requires a new format version.

Authenticate before parsing plaintext. Wrong passwords and authentication
failures return the same error. AEAD authenticates the payload and suite; it
does not prevent replay of an older valid version 2 envelope or bind a vault to
a particular server/configuration ID.

The [version 2 fixture](../src/fixtures/tabius-vault-v2.json) was generated with
argon2-cffi (`hash_secret_raw`, Argon2id v19), Python cryptography's HKDF-SHA256,
and libsodium's `crypto_aead_xchacha20poly1305_ietf_encrypt`. It includes a
synthetic Unicode password, plaintext, intermediate Argon2 and HKDF outputs, and
a complete envelope for independent implementations. Fixture salt bytes are
`00` through `0f`; nonce bytes are `10` through `27`. The encrypted plaintext
uses the fixture's key insertion order and compact UTF-8 JSON without ASCII
escaping, spaces, or a trailing newline. These deterministic values are only
for tests.

## Compatibility and migration

The `modern` mode creates version 2 whole-configuration vaults, including when
the remote configuration has no vault. It can read version 1 vaults and upgrades
them on a successful sync using the supplied password. It never retries a
failed version 2 decrypt as version 1. Once a destination has successfully synced
in modern mode, its locally saved mode also blocks subsequent plaintext or
version 1 replacements.

The `tabby` mode is explicitly labeled legacy and not recommended. It preserves
version 1 PBKDF2-HMAC-SHA512 (100,000 iterations, 8-byte salt) and AES-256-CBC
(16-byte IV, PKCS#7 padding, 32-byte key used directly from PBKDF2). Its vault
fields remain `version`, `keySalt`, `iv`, and `contents`. Both legacy
whole-configuration and secrets-only forms are retained. This mode never
downgrades version 2 vaults. Existing saved connections without an `encryption`
setting use compatibility mode; new destinations default to modern.

Choose modern mode only when all clients sharing that configuration understand
version 2. The server can store either format without decrypting it.

### Migration between servers

**Settings → Migrate from Tabby-Web** transfers a selected configuration from a
legacy Tabby-Web server to a Tabius sync server using their separate tokens.
Both servers use the HTTP contract above;
the destination configuration must already exist. Migration reads and decrypts
the source and serializes the complete document with modern encryption and a
separate destination vault password. It preserves all configuration and vault
fields, including secrets unsupported by the desktop workspace. It never
projects the data through the local host/key model or writes to the source.

The preview returns only counts and a SHA-256 fingerprint binding the canonical
source and destination endpoints to their content, modification timestamps, and
ETags. Migration requires this fingerprint, rejects changed snapshots, and reads
both configurations again after encryption and before the destination PATCH.
The PATCH includes the destination's ETag when available. Conditional writes
are still required to close the final read/write race on the destination. The
source can change after its final read; migration copies the reviewed snapshot.
Canonical endpoint equality is rejected to prevent replacing the source itself;
different URLs that alias the same server/configuration cannot be detected.

After PATCH succeeds, the client fetches the destination again and compares the
SHA-256 digest of its content with the encrypted content it uploaded. Only an
exact match reports migration success. An unavailable or mismatched read-back
reports that the upload finished but verification failed; the source remains
untouched. This verification does not prevent later edits by other clients.

The user explicitly reviews replacement of the destination's existing content.
Migration does not merge destination data, create server configurations, or
change the desktop workspace or its sync baselines. Subsequent synchronization
with the destination follows the usual workspace merge rules.
