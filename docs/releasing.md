# Releasing Sutra

How a release is cut, and the two things it still cannot do without a secret
only the project's owner can create.

## Cutting a release

1. Bump the version in the three files that carry it — `package.json`,
   `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`. They must agree; the
   release workflow refuses to publish if they do not, because an installer
   whose internal version differs from the tag on its download page installs as
   one version and reports itself as another.
2. Write `docs/releases/vX.Y.Z.md`. It becomes the release notes verbatim.
   Without it the workflow falls back to generated notes and says so.
3. Either push a tag, or run the **Release** workflow from the Actions tab and
   give it the tag:

   ```
   git tag v0.2.0 && git push origin v0.2.0
   ```

   The workflow runs the full Windows checks, builds the installers, and
   attaches them to a public release. The dispatch route exists because pushing
   a tag needs push rights on tag refs, which not everyone who should be able
   to cut a release has.

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

Once you have one, the signing step goes in `.github/workflows/release.yml`
between **Build** and **Publish**, and the secrets go in the repository's
Actions secrets. Tauri signs via its `bundle.windows.signCommand` hook or by
running `signtool` over the built artifacts; the details depend on which of the
two routes above you take, so they are deliberately not guessed at here.

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
