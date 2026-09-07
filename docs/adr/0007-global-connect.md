# ADR 0007 — Global Connect: the public site

Status: accepted; sections 1-8 built, section 9 (the meat) deliberately open
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
to say Phonix; that split is deliberate and §9 does not promise to close it.

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
site sets no cookie at all (§10), so there is nowhere to keep a preference even
if one were wanted. The trade is real: a visitor arrives in English and switches,
rather than arriving in their own language.

Two details that only show up once a second language exists. The switcher offers
*this page* in the other language rather than the home page, because one that
always lands on the front page loses somebody's place every time it is used. And
`<html lang>` is taken from the catalog that actually rendered rather than from
the address — they agree today, and the difference is the entire value of the
attribute, since a fallback would otherwise serve English while telling a screen
reader to pronounce it as Chinese.

## 8. What is built

* The crate, the binary, `[site]` in the configuration, and the checks on it.
* `phonix-limit`, with `phonix-server` moved onto it.
* The frame: base template, header, footer, 404, the two assets and the build
  script that hashes them (`tools/build-site-assets.mjs`, output committed —
  ADR 0005 §3's arrangement, and for the same reason).
* Five pages: home, product, pricing, about, contact.
* Two languages, the switcher, and the `hreflang` links — §7.

## 9. What is deliberately not built yet

The meat. This is a skeleton with a real home page on it, and the following are
named here so that adding them is a decision rather than a drift:

* **Per-app pages.** One page per `app-*` crate — Books, Inventory, People —
  each with the screens it actually has. Blocked on nothing except having
  something true to say about each.
* **Real pricing.** The page renders a shape; the numbers in it are placeholders
  and are marked as such in the template. `desk.trial_days` is the only number
  in this estate that currently means anything commercially.
* **Documentation and a changelog.** Both want a content pipeline (Markdown at
  build time), which is a dependency and therefore a decision.
* **German and French.** The machinery is built and the product already
  promises both, so each is one `de.rs` beside `en.rs`, one arm in
  `i18n::strings`, and its code in `i18n::has_catalog`. Nothing else changes,
  and until then those codes 404 rather than pretending — §7.
* **Anything that collects.** See §5. If a form is ever wanted, it is a
  reversal of that section and should be argued there rather than added quietly.

## 10. What it will not do

It will not sign anybody in, hold a session, or set a cookie. It has no
identity, no `Caller`, no equivalent of `DeskCaller`, and nothing to
authenticate against — §2 removed the database that would hold an account.

A visitor who needs to be *somebody* has left the site. That boundary is what
keeps this crate the cheapest thing in the estate, and it is the one to defend
when a feature request would cross it.
