# Releasing Sutra

How a release is cut, and the two things it still cannot do without a secret
only the project's owner can create.

## Cutting a release

1. Bump the version with `npm version X.Y.Z --no-git-tag-version`, which does
   `package.json` and `package-lock.json`, then by hand in
   `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and the `sutra` entry in
   `src-tauri/Cargo.lock`. The lockfiles are not optional: `npm ci` and a
   `--locked` cargo build both refuse a lockfile that disagrees with its
   manifest. The release workflow refuses to publish unless the first three
   agree with the tag, because an installer whose internal version differs from
   the tag on its download page installs as one version and reports itself as
   another. `the_version_shown_is_the_version_the_installer_carries` catches
   the same disagreement at commit time.
2. Write `docs/releases/vX.Y.Z.md`. It becomes the release notes verbatim.
   Without it the workflow falls back to generated notes and says so.
3. Either push a tag, or run the **Release** workflow from the Actions tab and
   give it the tag:

   ```
   git tag v0.2.0 && git push origin v0.2.0
   ```

   The workflow runs the full Windows checks, builds the installer, and
   attaches it to a public release. The dispatch route exists because pushing a
   tag needs push rights on tag refs, which not everyone who should be able to
   cut a release has.

## One installer

From v0.3.0 a release carries `Sutra_<version>_x64-setup.exe` and nothing else.
The `.msi` was dropped because two installers for one application is a choice
the person downloading has no basis to make, and this is a personal project
with no fleet deployment to serve — which is the one thing an `.msi` is
genuinely better at. `--bundles nsis` in the build step is what decides it;
`tauri.conf.json` says the same so a local `npm run tauri:build` produces what
CI does.

Bringing the `.msi` back is adding `msi` in both places. Nothing else in the
release path knows the difference.

## Not done yet: code signing

Every installer Sutra publishes is unsigned, so Windows SmartScreen warns on
first run and the user has to click through _More info → Run anyway_. For
something handed to a colleague, that is the largest single piece of friction
there is.

Fixing it is a purchase and an identity check, not a code change:

- **An OV code-signing certificate** from a CA (DigiCert, Sectigo and others).
  Costs a few hundred a year, and since 2023 the private key must live on a
  hardware token or in a cloud HSM — which means CI cannot sign with it unless
  the CA offers a cloud signing service.
- **Azure Trusted Signing** is the cheaper, CI-friendly route: Microsoft holds
  the key, you authenticate from the workflow. Eligibility rules apply to how
  long the legal entity has existed; check the current terms.

An EV certificate additionally clears SmartScreen's reputation check
immediately. An OV one builds reputation over time and downloads.

**The workflow is already wired for it.** The build step passes
`TAURI_WINDOWS_SIGN_COMMAND` through from a repository secret named
`WINDOWS_SIGN_COMMAND`. Tauri runs that command over each bundled artifact when
it is set, and does not sign when it is not — so nothing changes until you add
the secret, and every build says in its log which of the two it was.

To turn signing on, add one Actions secret, `WINDOWS_SIGN_COMMAND`, holding the
command your CA gave you with `%1` where the file goes. For Azure Trusted
Signing that is a `trusted-signing-cli` invocation; for a cloud HSM it is that
vendor's `signtool` wrapper. Nothing else in the repository has to change.

The exact command is deliberately not guessed at here: it differs per CA, and a
plausible-looking wrong one would fail at release time.

**Nobody should generate this key but you.** A signing key is an identity. One
created inside an automated session, or pasted into a chat log, is compromised
from the moment it exists.

## Not done yet: automatic updates

Sutra tells you a new version exists — **Settings → Version → Check for
updates** — but it cannot install one. That check is a button and never a
timer, because the app promises that nothing leaves your machine unless you
turn it on, and a background request to GitHub would make that quietly untrue.

Real auto-updating needs Tauri's updater plugin, which requires its own signing
keypair, separate from code signing. The steps, in order:

1. Generate the keypair. **Run this yourself** — for the same reason as above:

   ```
   npm run tauri signer generate -- -w ~/.tauri/sutra.key
   ```

   It prints a public key and writes a private one. Keep the password.

2. Add the private key and its password to the repository's Actions secrets as
   `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

3. Add the plugin: `tauri-plugin-updater` in `src-tauri/Cargo.toml`,
   `@tauri-apps/plugin-updater` in `package.json`, and
   `.plugin(tauri_plugin_updater::Builder::new().build())` in `main.rs`.

4. Configure it in `src-tauri/tauri.conf.json`, with the **public** key from
   step 1:

   ```json
   "plugins": {
     "updater": {
       "pubkey": "<the public key printed in step 1>",
       "endpoints": [
         "https://github.com/vermasaksham/Sutra-Windows/releases/latest/download/latest.json"
       ]
     }
   }
   ```

5. In the release workflow, build with those two secrets in the environment so
   Tauri emits a `.sig` beside each installer, and attach `latest.json` to the
   release alongside them.

Step 4 is why this is not already wired up: `pubkey` cannot be filled in with a
placeholder. A config carrying someone else's key would reject every update
signed with yours, which is worse than no updater at all — it fails at the
moment a person is trying to install a fix.
