import type { KeyStorage } from "../vault/api";

/**
 * Where a stored API key actually lives, said plainly.
 *
 * This replaced a sentence that always read "stored in plain text in the
 * app's config file". That was true before v0.2.1 and is now true only on a
 * platform with no credential store — and a security notice that is wrong in
 * either direction is worse than none, because one teaches the reader to
 * ignore it and the other tells them they are safe when they are not.
 */
export default function KeyLocation({
  storage,
  variable,
}: {
  storage: KeyStorage;
  variable: "ZOTERO_API_KEY" | "ANTHROPIC_API_KEY";
}) {
  const env = <code className="font-mono text-xs">{variable}</code>;

  switch (storage) {
    case "environment":
      return <>The key comes from {env}, so Sutra stores nothing at all.</>;
    case "keychain":
      return (
        <>
          The key is kept in this computer&rsquo;s credential store, not in
          Sutra&rsquo;s settings file. Setting {env} instead stores nothing at
          all.
        </>
      );
    case "config-file":
      return (
        <>
          <strong>This key is stored as plain text</strong> in Sutra&rsquo;s
          settings file, readable by anything running as you — there is no
          credential store available here. Setting {env} instead stores nothing
          at all.
        </>
      );
    case "none":
      return (
        <>
          A key typed here goes into this computer&rsquo;s credential store
          where there is one, and into Sutra&rsquo;s settings file as plain text
          where there is not — it says which once saved. Setting {env} instead
          stores nothing at all.
        </>
      );
  }
}
