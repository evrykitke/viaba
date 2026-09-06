# What a workspace starts with

One file per app, named after its `app_id`: `books.toml` holds the rows the
`books` app seeds. An app with nothing to seed needs no file, and a missing file
is not an error.

These are **defaults, not settings** — the same rule as `config/numbering/`.
Enabling an app inserts the rows; from that moment the workspace owns them. Every
insert is `ON CONFLICT DO NOTHING`, so a redeploy can neither put back a row
somebody deleted nor overwrite one they edited. A default is what a workspace
starts with, not what it is held to, which is exactly why this is a file that
gets reviewed in a pull request rather than a table somebody edits in production.

## Why an app ships defaults at all

An accounting app whose first screen is "you have no chart of accounts, create
one" has shipped a blank page and called it flexibility. Almost nobody wants to
design a chart of accounts, and the people who do want to are the ones who will
change it anyway.

So the point of these files is that the setup checklist on an app's home page is
already green on the first morning, without anybody having typed anything. Where
it cannot be — nobody can guess an opening balance — the checklist is what stops
that being discovered by an accountant in March.

## The shape is the app's, not this directory's

`config/numbering/` can describe its own format because every app's number series
look alike. Defaults do not: a chart of accounts and a set of stock adjustment
types have nothing in common but the directory they live in.

So `phonix_config::defaults` knows only how to find a file and parse it into
whatever type the calling app asks for, and the *rules* about what makes one
valid live with the app:

| File | Shape | Checked by |
| --- | --- | --- |
| `books.toml` | `app_books::account::DefaultChart` | `DefaultChart::check` |

Validation runs when the file is read, before a row is written. A broken file
should stop a deployment, where somebody is watching — not half-install a chart
that an accountant then unpicks by hand in a live workspace.

## `books.toml` — the chart of accounts

```toml
[[account]]
number      = "2010"                          # required, unique (case-insensitively)
name        = "Goods received not invoiced"   # required
type        = "goods_received_not_invoiced"   # required, an AccountType
description = "..."                           # optional, up to 500 characters
```

`type` is one of the variants of `app_books::account::AccountType`, spelled the
way the column stores it. It is what the software reasons about: it decides the
normal balance, whether the account closes into retained earnings at year end,
and whether a sub-ledger owns the account. `number` decides only the order things
are read in — a workspace is free to renumber the whole chart, and nothing may
break when they do.

The chart is **exhaustive** rather than a starter set, which is a deliberate
choice against discovering a missing account at the moment you are trying to post
something. An unused account costs one row and one line in a picker that is
searchable anyway; a missing one costs a posting failure and a phone call. See
ADR 0006 section 4, and the header of `books.toml` itself for the list of
accounts that are here precisely because the incumbents leave them out.
