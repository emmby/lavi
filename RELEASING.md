# Releasing Lavi

Lavi publishes pre-built binary packages across four primary registries from a single `v*` git tag via [`.github/workflows/release.yml`](./.github/workflows/release.yml):

| Ecosystem | Registry | Package Identifier | Install Command |
| :--- | :--- | :--- | :--- |
| **JavaScript / TypeScript (WASM)** | **npm** | `lavi-fsm` | `npm install lavi-fsm` |
| **Rust (`#![no_std]`)** | **crates.io** | `lavi` | `cargo add lavi` |
| **Python (`abi3` wheels)** | **PyPI** | `lavi` | `pip install lavi` / `uv add lavi` |
| **Java / Kotlin (Fat JNI JAR)** | **Maven Central** | `io.github.emmby:lavi` | `implementation("io.github.emmby:lavi:0.1.0")` |
| **C / C++ (Arduino IDE & PlatformIO)** | **GitHub Releases** | `lavi-embedded.zip` | Arduino IDE *Add .ZIP Library...* or PlatformIO `lib_deps` |

---

## 1. One-Time Registry & GitHub Secrets Setup

### 1.1 crates.io (`lavi`)
1. Sign in with GitHub at https://crates.io and verify your email address in **Account Settings**.
2. Go to **Account Settings → API Tokens → New Token**:
   - Name: `lavi-github-actions`
   - Scopes: `publish-new` and `publish-update`
3. Add the token to GitHub repository secrets (**Settings → Secrets and variables → Actions**) as:
   - `CARGO_REGISTRY_TOKEN`
4. *(Optional after first publish)*: On `https://crates.io/crates/lavi/settings`, enable **Trusted Publishing** for `emmby/lavi` workflow `release.yml`.

### 1.2 PyPI (`lavi` — Zero-Secret OIDC Trusted Publishing)
1. Sign in to https://pypi.org and go to **Your Account → Publishing** (`https://pypi.org/manage/account/publishing/`).
2. Under **Add a new pending publisher**, fill in:
   - **PyPI Project Name**: `lavi`
   - **Owner**: `emmby`
   - **Repository name**: `lavi`
   - **Workflow name**: `release.yml`
   - **Environment name**: *(leave blank)*
3. Click **Add**. No GitHub secret is needed—`pypa/gh-action-pypi-publish` authenticates via GitHub OIDC (`id-token: write`).

### 1.3 npm (`lavi-fsm` — Zero-Secret OIDC Trusted Publishing)
1. Sign in to https://www.npmjs.com and open `https://www.npmjs.com/package/lavi-fsm/access`.
2. Under **Trusted Publisher**, configure **GitHub Actions**:
   - **Organization or user**: `emmby`
   - **Repository**: `lavi`
   - **Workflow filename**: `release.yml`
   - **Publishing access**: Require two-factor authentication and disallow bypass 2FA tokens (recommended).
3. No `NPM_TOKEN` GitHub secret is required—`npm publish --provenance --access public` authenticates directly via GitHub OIDC (`id-token: write`).

### 1.4 Maven Central (`io.github.emmby:lavi` via Sonatype Central Portal)
1. Sign in with your `emmby` GitHub account at https://central.sonatype.com.
   - Verify under **Namespaces** that `io.github.emmby` is marked **Verified**.
2. Go to **View Account → Generate User Token** and add the credentials to GitHub repository secrets:
   - `MAVEN_CENTRAL_USERNAME`
   - `MAVEN_CENTRAL_TOKEN`
3. Generate and publish a PGP signing keypair using the included helper script:
   ```bash
   ./scripts/setup-maven-gpg.sh
   ```
   Add the printed values to GitHub repository secrets:
   - `MAVEN_GPG_PRIVATE_KEY`
   - `MAVEN_GPG_PASSPHRASE`

### 1.5 Embedded C/C++ (`lavi-embedded.zip` for Arduino IDE & PlatformIO)
No external secrets are required. The `build-embedded` job in `release.yml` automatically installs `espup`, cross-compiles `liblavi_c_cpp.a` for all 6 ESP32 architectures (`esp32`, `esp32s2`, `esp32s3`, `esp32c3`, `esp32c6`, `esp32h2`), packages `dist/embedded/lavi-embedded.zip` (containing `library.properties`, `library.json`, `build_rust.py`, headers, precompiled `.a` archives, and `examples/Stoplight/Stoplight.ino`), and attaches it to the GitHub Release.

---

## 2. Cutting a Release

1. **Synchronize all workspace manifests to the new version**:
   ```bash
   node scripts/sync-version.mjs 0.1.0
   node scripts/sync-version.mjs --check 0.1.0
   ```
2. **Run the full test suite locally**:
   ```bash
   npm test
   ```
3. *(Optional)* **Run a CI Dry-Run first**:
   Trigger the **Release** workflow manually in GitHub Actions with `dry_run: true` to verify all cross-compiled wheels, WASM bundles, and Maven Central bundles build cleanly before tagging.
4. **Commit, tag, and push**:
   ```bash
   git commit -am "chore(release): v0.1.0"
   git tag v0.1.0
   git push origin main --tags
   ```
