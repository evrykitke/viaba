# ADR 0007 — Global Connect: the public site

Status: accepted; sections 1-11 built, section 12 (the meat) deliberately open
Date: 2026-09-07

Every page this estate serves today is behind a sign-in box. `phonix-server`
answers a workspace host or the bare domain, and on the bare domain it shows a
sign-in form and a signup wizard. `phonix-desk` answers `console-desk.` and is
`noindex, nofollow` on every page because it suspends workspaces. There is
nowhere at all that says what the product *is*.

This record specifies that: `global-connect`, a third binary serving
`www.<base_domain>` and the apex, whose entire job is to be the reason somebody
reaches `/signup` — and to be cheap enough that being linked from somewhere busy
is not an incident.

The tagline is the requirement: **making good management software accessible to
everyone.** A site that takes four seconds to paint a headline has already
contradicted it.

---

## 1. It is a third binary, and the argument is ADR 0005's

[ADR 0005 §2](0005-phonix-desk.md) worked through second-binary-versus-second-
repository for Desk and landed on a workspace member. The same reasoning holds
here and the conclusion is the same, but one of Desk's three arguments does
**not** apply and it is worth saying which.

Desk's deciding argument was the migrations: it exists to answer "which tenants
are on which schema version", so it must move in step with the schema, so it
cannot live in a repository that versions separately. **The site has no such
tie.** It reads no database and knows no schema. On that axis alone a separate
repository — or a static generator, or a hosted page builder — would be fine.

What keeps it here is smaller and still sufficient:

1. **It shares the theme.** `style/theme.css` is already described in its own
   header as "the only file two applications share". A third application
   importing it is one line and no code. A site built elsewhere is a second
   palette that disagrees with the product's within a month, and the first thing
   a visitor does after the site is click into the product.
2. **It must name addresses this deployment actually serves.** "Get started"
   points at `/signup` on the bare domain, which is `localhost:3000` on a
   developer's machine and something else in production. §4 derives it from
   `[server]` rather than writing it down, and that derivation needs
   `phonix-config`.
3. **One repository, one release.** Same team, same box. The cost of a second
   repository is paid every release; the benefit was independent cadence, which
   nobody wants.

## 2. It depends on nothing that can be down

```
global-connect → phonix-config, phonix-limit, phonix-telemetry
```

No `phonix-db`. No `phonix-services`. No `phonix-cache`, no `phonix-messaging`,
no `phonix-storage`. No SMTP.

Desk's dependency list was already deliberately short and still reaches
Postgres, because reading the catalog is what Desk is *for*. The site is for
nothing of the sort, and that buys a property worth stating plainly: **the day
the database is down, the public site is up.** That is also the day traffic
arrives at it, because it is the address people already know.

It follows that the site cannot hold anything, and §5 makes that a rule rather
than an accident.

## 3. Server-rendered, and lighter than Desk

Askama compiles the templates into the binary, exactly as Desk does and for the
reasons written out in [`phonix-desk/src/html.rs`](../../crates/phonix-desk/src/html.rs):
escaping is the default, pages compose by `{% extends %}` rather than by passing
a rendered body string, and no template writes `|safe`. Nothing is read from
disk at request time, so the site is one artefact — copy the binary, run it.

**No `phonix-web`, and here the reason is stronger than Desk's.** Desk avoided
the product crate because a wasm panic freezes every handler on the page and
Desk is wanted precisely when the product is misbehaving. The site avoids it
because a marketing page is the cheapest thing an estate serves or it is doing
its job badly. The product's bundle is megabytes; this site's whole payload is
one stylesheet and one optional script, both hashed, both `immutable`, both
compiled in.

**Every page is complete without the script**, unchanged from ADR 0004 §6 and
ADR 0005 §3. The one script toggles a mobile navigation panel that is a
`<details>` element without it. A page that needs JavaScript to show its
headline is a page that shows nothing on a slow connection.

Consequently there is no dark-mode toggle. The site does not follow
`prefers-color-scheme` either: it commits to dark and writes `data-theme="dark"`
on `<html>`. Desk omits that attribute because it is a tool somebody keeps open
all day beside other windows; this is one composition with artwork in it, and a
composition that renders two ways is two compositions to keep good.

There *is* a language switcher, and it needs no script — see §7.

## 4. Where it is served, and what it points at

`www.<base_domain>` and the apex, bound to `127.0.0.1:3200` with nginx in front.
`www` is **already** in `tenancy.reserved_subdomains`, so no workspace can have
claimed it — the reservation was made for a different reason and happens to be
exactly what this needs.

Loopback rather than `0.0.0.0`, and `validate::check` refuses otherwise under
production unless `site.allow_public` says it is deliberate. This is Desk's rule
with a different consequence: a public bind answers whatever `Host` header it is
sent, and on a box that also serves workspaces that means the marketing site
answering for a tenant.

**The calls to action are derived, not written down.** `signup_url` and
`sign_in_url` are blank by default and built from `[server]` —
`ServerConfig::origin()` plus `/signup`. A developer's box gets
`http://localhost:3000/signup` and production gets its own, and neither had to be
maintained. They exist as overrides only for a deployment where the site and the
application are not one label apart.

`site.product_name` is configuration for the same reason: the name is in the
wordmark, every page title and half the sentences, and a rename should not be a
search across a directory of HTML. It is `Viaba` here while the crates continue
to say Phonix; that split is deliberate and §12 does not promise to close it.

## 5. It collects nothing

No form posts. Not a contact form, not a demo request, not an email capture. The
only `POST` route the site has is none.

This was a decision and not an omission. A form needs somewhere to put what it
receives, which is SMTP or the catalog — and either one hands back the
dependency §2 just removed, on the page most likely to be pointed at by a link
somebody else controls. It also needs spam defence that a rate limit is not.

So "Talk to us" is a `mailto:`, and every other call to action is a link to
`/signup`, where the product already knows how to create a workspace, validate
an address, and refuse a duplicate. The signup wizard is good; the site's job is
to end at its door.

`site.contact_email` empty renders **no link at all** rather than a button
addressed to nobody — the rule `[app.links]` already follows.

## 6. Rate limiting, and one implementation of it

Anonymous traffic here is the only traffic here. The ceiling is not about abuse
of an expensive operation — there are no expensive operations — but about a
crawler that has stopped being polite, and about the site being the cheapest
thing to point a flood at.

`phonix-server` already had a fixed-window limiter. It was private to that
crate, and everything around it — four tiers by path, an API call keyed on its
bearer token — is about the product's routes and means nothing here.

**So the counting moved and the policy did not.** `phonix-limit` is a new crate
with no dependencies at all: a counter and an expiry per key, a sweep so the map
cannot grow without bound, `Allow` or `Refuse { retry_after_secs }`.
`phonix-server` keeps `Tier`, `classify`, the middleware and the key derivation,
and puts the tier into the key. The site keeps one tier, one key, and one
allowance.

The alternative was a second limiter in this crate, and it was turned down for a
reason specific to this kind of code: a limiter that has quietly stopped
agreeing with itself is a security problem, and the copy that is subtly wrong is
the one nobody looks at.

**Assets are not counted.** One page view is the HTML plus a stylesheet plus a
script, so counting all three would spend a reader's allowance three times as
fast as the number suggests. In production nginx serves them and they never
arrive; in development they do.

**The key is `[security.rate_limit] client_ip_header`, reused.** Which header
carries the visitor's address is a fact about this deployment behind this proxy
and a second copy of it is a second thing to get wrong — the same call Desk
made about the whole `[security]` section. Only the numbers are the site's own,
because the shapes genuinely differ: the server's tiers are about a password
being guessed at and a database being created, and the site has neither.

## 7. A language is an address

The product speaks four languages at exact catalog parity. A front door speaking
one would be the first thing a German or Chinese visitor learned about the
product, so the site is translated too — **English and Chinese today**, with the
machinery for the rest.

Two languages and not four is the honest half-step, and the split is deliberate.
The *plumbing* is cheap and was built now because retrofitting it into finished
templates is the expensive version — the same argument
[`phonix_core::i18n::Direction`](../../crates/phonix-core/src/i18n/language.rs)
makes for carrying `dir` before components hard-code `ml-`/`mr-`. The *prose* is
the standing bill: every edit to a sentence is an edit per language, forever.
Chinese was written second rather than German because it is the language least
like the original — it sets far less ink for the same sentence and breaks
anywhere rather than at spaces, so a layout that survives it will survive the
two that are near-neighbours of English.

**The site offers what it has, not what the product has.** `i18n::has_catalog`
is the one place a language code appears in this crate; the switcher, the
`hreflang` links, the URL prefixes and the catalog lookup all read it. So
`/fr/pricing` is a **404** rather than English wearing a French address, which
is both the honest answer and the one a search engine does not punish.

### The strings are a struct, not a catalog

The product's `msg!` const-asserts a key into a JSON catalog loaded at boot.
That is right for thousands of short keys edited by translators. This is ninety
sentences of prose, and a missing one is a blank headline rather than a fallback
nobody notices — so each language is a `static` of one `struct`, and **a
language that has forgotten a sentence does not compile.** The arrays are
fixed-length for the same reason: a translation that describes two applications
where English describes three is a build failure.

The hole this does not close is a *stale* sentence. English is the original; a
catalog that still says last month's headline compiles perfectly. That is
written down in `en.rs` rather than solved, because solving it means a
translation workflow and this is a five-page site.

### The language is in the URL, and nowhere else

`/pricing` is English, `/zh/pricing` is Chinese. Not a cookie, not
`Vary: Accept-Language`, and no redirect off the front page.

A page that varies by header is a page a shared cache holds one arbitrary
version of, and these pages are cacheable precisely because they hold nothing
about anybody. A search engine needs one address per language to index. And this
site sets no cookie at all (§13), so there is nowhere to keep a preference even
if one were wanted. The trade is real: a visitor arrives in English and switches,
rather than arriving in their own language.

Two details that only show up once a second language exists. The switcher offers
*this page* in the other language rather than the home page, because one that
always lands on the front page loses somebody's place every time it is used —
amended by §14 for the one case this did not anticipate, a page that exists in
English only, where there is no other-language version to offer. And
`<html lang>` is taken from the catalog that actually rendered rather than from
the address — they agree today, and the difference is the entire value of the
attribute, since a fallback would otherwise serve English while telling a screen
reader to pronounce it as Chinese.

## 8. Pictures, and the one place bytes are allowed in

§3 says the site's whole payload is one stylesheet and one script. Screenshots
break that, and are worth it: nothing else on the page proves the product exists.

So there is a budget rather than a prohibition, and the build enforces it.
`crates/global-connect/artifacts/` holds them, `tools/site-artifacts.mjs` scans
it, and a file is **refused** — not warned about — if it is over 250 KB or under
1600px wide. The folder's README is the contract and also the list of which
screenshots are worth taking.

**They are compiled in, like the stylesheet.** The site stays one artefact, and
a route table built from known names has no path to traverse — which a
`ServeDir` over a directory does. The honest cost is that the binary grows by
the weight of the folder, which is exactly why the ceiling refuses.

**The filename is the wiring.** `inventory-items.webp` belongs to Inventory
because of its first segment, and a first segment naming no application fails
the build with the name printed. The alternative is a picture that silently
never appears, and nobody notices a missing image the way they notice a broken
build. The application list therefore exists twice — in `pages::APP_SLUGS` and
in the build script — and a test fails if they disagree.

**Dimensions are read out of the file at build time** and reach the `<img>`.
Both of them, always: one without the other is a page that reflows when the
picture lands, and the reflow is worst on the slow connection this site exists
to be kind to.

**The frame is the illustration's frame.** A real screenshot wears the same
window chrome the drawn mock already had — three dots, one caption. That shared
frame is the whole trick to a photograph and a drawing appearing on one page
without looking like two different sites. It is also why the home page's
illustration is a *fallback*: the first screenshot that exists replaces it, and
until then the page is complete without one.

Alternative text is one pattern per language with `{app}` and `{screen}` in it,
not a sentence per picture. A caption per screenshot would be a translation job
every time somebody took one, which is the surest way to end up with a folder of
images described in English on a Chinese page.

## 9. What a search engine is told

The site is the only surface in the estate that wants to be found — Desk is
`noindex` on every page — so this is not decoration.

* **A canonical link on every page.** Not because a page answers at several
  addresses; each answers at exactly one. Because a shared link acquires
  tracking parameters, and without this each variant competes with the original.
* **`hreflang` in absolute form**, plus `x-default` on English. A relative
  `hreflang` is legal and useless: the tag exists to tell a crawler two
  *addresses* are one page.
* **`sitemap.xml`**, generated from `pages::PATHS` × the offered languages, with
  the alternates spelled out on every entry. One list feeds the router and the
  sitemap, so a page that exists cannot be missing from it and an entry cannot
  404. `robots.txt` names it, absolutely — which is one more reason
  `site.public_url` is refused blank under production.
* **Open Graph and a Twitter card**, and `og:image` only when
  `artifacts/og-image` exists at exactly 1200×630. A card promising a picture it
  does not have renders worse than no card.
* **JSON-LD**: an `Organization` and a `WebSite`, which is the honest amount.
  Deliberately **no** `Product` with an `offers` price, because the prices are
  placeholders (§12) and marking up a made-up number is how a search engine ends
  up quoting it back.

**This is the one place a template writes `|safe`,** and it is safe by
construction rather than by judgement. Askama's `json` filter is documented to
emit no chevrons, apostrophes or ampersands, so `</script>` cannot appear in its
output; `|safe` only stops the HTML escaper turning valid JSON into `&#34;`,
which a script data block would not decode. The rule §3 inherited from Desk —
"no template writes `|safe`" — is now "one does, and here is why it cannot be
the hole the rule was guarding".

`site.public_url` is new configuration and is **required under production**,
because none of the above can be derived from `[server]`: that is where the
application lives, and this crate exists precisely because the two are different
hosts. Blank falls back to `http://localhost:<listen port>`, which is right on a
developer's machine and is refused everywhere else — a canonical link pointing
at localhost tells a crawler the real page is somewhere it cannot reach.

## 10. Solutions, and why it is one page

One page at `/solutions`, reached by a plain link in the top bar.

**Amended 2026-09-19: the mega menu is gone.** It was a `<details>` panel of the
six industries and the three applications, and it was desktop-only - below `md`
it degraded into the same links the phone's navigation already carried. A panel
that half the visitors never see, duplicating the page it opens onto, is a
second navigation to keep in step with the first. Solutions is now a link
beside Product, Pricing, About and Contact at both widths. The single list in
the catalog still feeds the page's own index of sections, which is where the
guarantee that a link cannot point at a missing section now lives.

**Six anchors on one page, not six pages.** An index of links has to lead
somewhere, and six links to six pages that have not been written is six 404s.
One page with anchored sections is the version that is true today, and splitting
a section out later keeps every link somebody has already shared —
`/solutions#health` can redirect. The anchors are in `pages::INDUSTRY_SLUGS`
rather than in the catalogs, because a slug appears in a URL: a link sent to a
colleague who reads the site in Chinese has to land in the same place.

### Two things that only showed up here

The application marks became **flat multi-colour icons** — the Google-catalogue
idea in this site's own hues, drawn on a 48 grid because an isometric box on a
24 grid lands its vertices on half pixels. They are possible at all only because
`fill` is a presentation attribute rather than a `style` rule; the policy is
`style-src 'self'` with no `unsafe-inline`, and Desk hit the same wall and
reached the same answer for its colour swatches. The colour reaches the tile
behind the mark through a class per application, for the same reason.

And **the handwriting is deliberately not load-bearing.** `.handwritten` is used
for asides only. A Latin hand has no CJK glyphs, so `_generated-fonts.css`
declares a `unicode-range` and a Chinese page falls through — glyph by glyph —
to a system brush face. With no font dropped into `fonts/` at all it degrades to
the browser's `cursive`, which is worse and is not broken. The hand-drawn
*marks* — the underline under the headline — are SVG rather than type, which is
why they are safe on a Chinese headline where the font is not.

## 11. What is built

* The crate, the binary, `[site]` in the configuration, and the checks on it.
* `phonix-limit`, with `phonix-server` moved onto it.
* The frame: base template, header, footer, 404, the two assets and the build
  script that hashes them (`tools/build-site-assets.mjs`, output committed —
  ADR 0005 §3's arrangement, and for the same reason).
* Six pages: home, solutions, product, pricing, about, contact.
* Two languages, the switcher, and the `hreflang` links — §7.
* The screenshot pipeline and its rules — §8. The folder is empty, and
  every page renders without a picture in it.
* Canonical links, sitemap, Open Graph and JSON-LD — §9.
* The Solutions page, six industries, reached by a plain link — §10.
* Coloured application marks, and the handwriting for asides — §10.

## 12. What is deliberately not built yet

The meat. This is a skeleton with a real home page on it, and the following are
named here so that adding them is a decision rather than a drift:

* **Per-app pages.** One page per `app-*` crate — Books, Inventory, People —
  each with the screens it actually has. Blocked on nothing except having
  something true to say about each.
* **Real pricing.** Amended 2026-09-19: `/pricing` no longer renders a shape
  with placeholders in it. Nothing has been priced and nothing is being charged,
  so the page says that and carries no plan grid and no numbers — a marked-up
  guess is still a guess, and a visitor reads the number rather than the
  footnote under it. That is the current answer rather than a page waiting to be
  filled in; when there is a real price it is published there, and the promise
  that it appears before anybody is asked to pay one is on the page itself.
  `desk.trial_days` remains the only number in this estate that currently means
  anything commercially, and it is not quoted here.
* **Documentation and a changelog.** Both wanted a content pipeline, which was
  a dependency and therefore a decision. Settled 2026-09-19 in §14: Markdown in
  `content/`, parsed by a `build.rs` into `OUT_DIR`, parsers in
  `[build-dependencies]` so the binary links neither. The pipeline is decided
  and not yet built; the 114 pages that come through it are the items after
  it.
* **German and French.** The machinery is built and the product already
  promises both, so each is one `de.rs` beside `en.rs`, one arm in
  `i18n::strings`, and its code in `i18n::has_catalog`. Nothing else changes,
  and until then those codes 404 rather than pretending — §7.
* **Anything that collects.** See §5. If a form is ever wanted, it is a
  reversal of that section and should be argued there rather than added quietly.

## 13. What it will not do

It will not sign anybody in, hold a session, or set a cookie. It has no
identity, no `Caller`, no equivalent of `DeskCaller`, and nothing to
authenticate against — §2 removed the database that would hold an account.

A visitor who needs to be *somebody* has left the site. That boundary is what
keeps this crate the cheapest thing in the estate, and it is the one to defend
when a feature request would cross it.

## 14. The content pipeline

§12 named Markdown at build time as "a dependency and therefore a decision".
This is the decision. It is written against the 114 files as they actually are,
read off the box on 2026-09-19, rather than against a summary of them.

### It is parsed by cargo, into `OUT_DIR`

A `build.rs` in this crate walks `crates/global-connect/content/`, validates
every file, renders each body to HTML, and writes one generated module to
`OUT_DIR` that the crate `include!`s. Nothing generated is committed.

Three alternatives, and why each loses:

* **Node, like the stylesheet.** §8's precedent is real but it is for *assets*,
  and `build-site-assets.mjs` says why: node is needed only to change how the
  site looks. Content is edited far more often than a stylesheet, and the
  generated file would be a quarter of a megabyte of rendered HTML in the tree —
  so every typo fix would arrive as an unreadable diff beside a one-line one.
* **At boot.** §3 says the site is one artefact: copy the binary, run it. A
  directory read at startup is a second thing to deploy, a second thing to get
  wrong, and a path that has to exist for the process to serve its front page.
* **At request time.** §2.

What this buys is that `cargo` remains the only tool that builds and deploys
this crate — ADR 0005 §3's rule, and §8's — while the content stays readable
`.md` in the tree rather than a generated artefact somebody has to regenerate
before their edit is visible.

### The parsers are build dependencies, and the binary links neither

`pulldown-cmark` for the body, with the tables extension because the knowledge
base uses GFM tables and CommonMark has none; a YAML parser for the front
matter, which is genuinely YAML — an article's `kbLinks` is a list of maps, not
a flat key and value.

Both sit in `[build-dependencies]`. **§2's list is untouched**: what reaches the
binary is `&'static str`, and nothing that can be down was added. The crate that
owns them is this one, because nothing else in the estate renders Markdown and
`phonix-core` compiles to wasm and takes no build dependencies. The day a second
crate wants Markdown is the day to move the parser, not before.

The honest cost is the binary. The three trees are about 277 KB of Markdown and
render to rather more HTML. That is well inside the shape §8 already accepted —
its ceiling is 250 KB for a *single* screenshot — but it is growth, and the day
the knowledge base is ten times this size is the day to revisit this section
rather than to quietly carry a megabyte.

### Three front-matter types, not one

The trees do not share a shape, and the survey is what says so rather than a
guess:

| Tree | Files | Front matter |
| --- | --- | --- |
| `articles/` | 13 | title, description, keywords, author, publishedAt, updatedAt, excerpt, readTime, kbLinks |
| `kb/` | 53 | title, description, author, and keywords on 48 of them |
| `learn/` | 43 | title, description, keywords, author, youtubeId, duration, publishedAt, order, level, published |

So `Article`, `KbDoc` and `Lesson`, each with exactly the fields its tree
carries. One union type would be a struct of mostly-`None`, and a missing
article date would then be indistinguishable from an absent optional — which is
the whole point of a type here. `KbDoc::keywords` is the one genuinely optional
field, and it is optional because five files say so.

**The build refuses rather than warns**, the same rule §8 applies to a
screenshot, and for the same reason: nobody notices a page that silently never
appears the way they notice a build that will not finish.

`_category.yaml` sits beside the articles categories and `_topic.yaml` beside
the learn topics, carrying a title, a description, an emoji and — for a topic —
an `order`. **`kb/` has no such file**, which the earlier summary of this
content got wrong; its categories are the directory names until somebody writes
one. The `icon_color` and `icon_bg` in those files are light-theme hex values
from the site being replaced and are not carried over: §3 commits this site to
dark, and a `#faf5ff` background on it is a white rectangle. The emoji comes
across; the colours do not.

`kbLinks` are checked. An article naming a category and slug that no `kb/` file
answers to fails the build with both printed — the same discipline as §8's
filename wiring, and the same reason: a dead link inside the content is exactly
what nobody re-reads.

### The body starts at `##`

A page's `<h1>` is the front matter's `title`, always, because `<title>`, the
Open Graph title and the heading are one sentence and three sources for it are
two too many.

Every `articles/` and `kb/` body opens with its own `# ` and no body anywhere
carries a second one — 66 files, one heading each, checked. So the build
**drops a leading H1 whatever it says** and fails on any later one. Dropping it
only when it matches the title would fail 42 of 53 knowledge-base files, whose
title ends in "— Evrykit" and whose heading does not.

### A draft is `published: false`, and only `learn` has the key

The build leaves an unpublished file out of the generated tables, and therefore
out of the router and out of the sitemap — §9 generates the sitemap from the
same list that builds the router, and a page that is not served must not be
advertised.

Absent means published. Only the 43 `learn/` files carry the key and every one
of them says `true`; defaulting the other way would drop 66 files that have
never had it.

### The content is English, and that is a 404 rather than a fallback

§7 says the site offers what it has. These 114 files are English only, so
`/zh/articles/...` **404s**. English at a Chinese address is the thing §7
refused for `/fr/pricing` and it is no more honest here.

**This amends §7 in one place.** That section says the switcher offers *this
page* in the other language, because one that always lands on the home page
loses somebody's place. On a content page there is no other-language version to
offer, so it offers the home page in that language — the exception §7's rule
did not anticipate, written down here rather than discovered as a 404 in a
switcher. A content page emits no `hreflang` alternate for a language it does
not have, for the same reason.

### What does not come across

`scripts/` (4 files) and `youtube/` (1 Markdown file and 11 `.txt`) are video
production material, not pages. They are not part of this pipeline and are not
brought into the tree. They stay on the box, and the commit that brings the
content across says so rather than dropping them quietly.
