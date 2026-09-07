# App artifacts — the screenshots

Drop screenshots in this folder. `node tools/build-site-assets.mjs` hashes each
one, reads its dimensions, and writes `src/artifacts.rs`; the site picks them up
with no further edit. An empty folder is fine — every page renders without a
single image in it, and that stays true.

---

## The name is the wiring

```
<app>-<screen>.webp
```

The part **before the first hyphen** is the application slug and must be one the
site knows: `books`, `inventory`, `people`. Everything after it is the screen,
and hyphens in it are fine.

```
inventory-items.webp
inventory-item-form.webp
inventory-warehouses.webp
books-chart-of-accounts.webp
people-departments.webp
```

A file whose first segment names no application **fails the build** with the
name printed. That is deliberate: the alternative is a screenshot that silently
never appears, and nobody notices a missing picture the way they notice a broken
build.

Order within an app is alphabetical by screen, so prefix with a digit if you
want a particular one first: `inventory-1-items.webp`.

## The rules the build enforces

| | |
| --- | --- |
| **Format** | `.webp` or `.png`. WebP unless you have a reason. |
| **Width** | at least 1600px, so it is still sharp on a 2× display. |
| **Weight** | 250 KB or the build refuses. A page with four of these is already heavier than the rest of the site put together. |

Everything below is not enforced, and matters more.

## How to take one

**Dark theme, violet accent.** The site is dark and commits to it. A light
screenshot on this page looks like a hole punched in it. Set the workspace to
dark and leave the accent on the default violet, so the product's brand colour
in the picture is the same one in the page around it.

**1600 × 1000, near enough.** A 16:10 window. Wider than that and the text in
the picture is too small to read at the size it is displayed; taller and the
frame dominates the band it sits in.

**Hide the browser.** No URL bar, no bookmarks, no tabs, no extension icons. The
site draws its own window chrome around the image — three dots and a caption,
the same frame the illustration on the home page uses — so a second set of real
chrome inside it looks like a screenshot of a screenshot.

**A real workspace with plausible data.** Empty grids sell nothing and neither
do three rows of "test test test". Ten to twenty rows, real-looking codes,
believable quantities. Nothing that is a real customer, a real person's name, or
a real email address — this is a public web page and it is indexed.

**Use the same demo workspace for every shot**, so the caption bar and the
sidebar agree from one picture to the next. `acme` is what the drawn
illustration on the home page already says.

**Nothing transient.** No toast, no open dropdown, no half-typed field, no
loading spinner, no red validation message — unless the point of that particular
shot is the thing on screen.

## Which ones to take

Ordered by how much each one earns its weight. The first three are worth having
before any of the rest.

### Worth it first

1. **`inventory-items.webp`** — the items grid, ~15 rows, a filter or two
   visible in the toolbar. This is the picture of "dense where it should be
   dense", and it is the one that replaces the drawn illustration on the home
   page. Take this one even if you take nothing else.
2. **`books-chart-of-accounts.webp`** — the tree, expanded two levels, with the
   account roles showing. It is the fastest way to say "this is real
   accounting" to somebody who knows what they are looking at.
3. **`inventory-item-form.webp`** — the tabbed item form on the General tab,
   with the category lookup and the description editor filled in. It shows the
   product is a working application and not a set of tables.

### Worth it next

4. **`books-journal-entry.webp`** — a balanced entry, both sides visible, the
   totals agreeing at the foot. Proof of the claim the copy makes.
5. **`inventory-warehouses.webp`** — the warehouse list including the default,
   showing the receipt/delivery step counts.
6. **`people-departments.webp`** — the tree with cost centres marked. The People
   section is thin and a picture carries it.
7. **`inventory-locations.webp`** — the location tree under a warehouse, so the
   internal/vendor/customer types are visible.

### Only once the pages exist to hold them

8. `books-periods.webp` — the financial year with its periods, one closed.
9. `inventory-categories.webp` — categories with costing and valuation set.
10. `inventory-item-variants.webp` — the variants tab of an item.

## Two things this folder is not for

**Not the logo, the favicon or any decoration.** Those are inline SVG in the
templates, which is why the content security policy allows no external image and
why the crate ships no picture files. Artwork that is markup cannot be a
broken-image icon.

**Not a photograph of anybody.** See ADR 0007 §5 — this site collects nothing,
and it should not publish anything either.
