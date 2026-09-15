# Backlog

The queue `/advance` works through. One item per iteration, top of `## Next`
first. Everything here is a *claim about what should be true when the item is
done*, not a task description — the difference matters, because the loop uses
it as the acceptance test.

## Format

```
- [ ] `crate-name` Subject line, as a noun phrase
      why: the reason this is worth doing, in one or two sentences
      touch: crates/.../file.rs, crates/.../other.rs      (optional hint)
      done: what is true afterwards that is not true now  (the acceptance test)
```

Only `why:` is required. `touch:` is a hint, not a fence — the loop may find
the work lives elsewhere and will say so. An item with a `blocked:` line is
skipped until that line is removed.

Keep items small enough that one of them is one commit. If an item needs three
commits it is three items.

---

## Next

## Blocked

<!-- Items waiting on something outside the loop's reach. Each carries a
     `blocked:` line saying what it waits for. -->

- [ ] `phonix-web` A grid that opens already narrowed
      why: `GridState::new` starts with no filters and `default_value()` is read
           only by tests, so a screen cannot open on anything but everything.
           The employees list wanted to open on current staff and could not say
           so; the comment asking for it has been removed rather than honoured.
      touch: crates/phonix-web/src/ui/table/{state.rs,config.rs}
      done: a `GridConfig` can declare a filter's opening value, `GridState`
            seeds it, and the twenty-odd `default_value() == ""` tests still
            describe the grids that did not ask for one.
      blocked: nothing would call it but the employees grid, so building it
               means also deciding that the staff list opens on current staff
               rather than everybody - a change to what a screen shows, on the
               authority of one comment that had never been implemented. Say
               whether you want that default and it goes in; say no and this
               item should be deleted rather than left here.

## Done

<!-- The loop appends here with the commit sha. Newest first. -->

- [x] `workspace` The workspace adopts rustfmt
      Decided: adopt rather than drop the gate. `cargo fmt --all` in one sweep,
      206 files, no `rustfmt.toml` - default rustfmt is what the gate runs and
      measuring showed no width makes this code a no-op anyway. `check.ps1`'s
      fmt gate passes from here, and a scoped `cargo fmt -p <crate>` now touches
      only what was just edited.

- [x] `phonix-web` The chart of accounts grid, last of the four that grow
      All four are now paged. Class and postable are derived from the account
      type rather than stored, so the store sorts and filters them through
      expressions generated from `AccountType::ALL` - the rule stays in the
      enum rather than being copied into SQL.

- [x] `phonix-web` The agreement tests for the users and locations grids
      Both now assert that every column offering a sort or a search is one the
      store actually handles, and the locations grid asserts every kind it
      offers can be bound. Typechecked, not executed - the `phonix-web` test
      binary is OOM-killed on this machine.

- [x] `phonix-web` The employees filter that opened on a choice it did not apply
      "All" is first now, which is what the screen already did, and the grid
      carries the agreement tests its neighbours have. Opening genuinely
      narrowed needs a capability the kit does not have; queued rather than
      faked.

- [x] `phonix-web` The locations grid, third of the four that grow
      The one tree among them. Tree order and depth now come off the stored
      path via `string_to_array`, so a page can be drawn without the rows
      above it; `in_tree_order` stays for the forms, which still read the
      whole tree. Both filters moved to SQL, `on_hand` mapping to `internal`.

- [x] `phonix-services` `directory::find`, deleted rather than fixed
      The item said a user's screen read the whole table through it. That was
      wrong: nothing called it at all, and `directory::card` already reads one
      account by id through `store::card`. So the whole-table read is gone by
      deletion, and no second single-row reader was written.

- [x] `phonix-web` The users grid, second of the four that grow
      Search and sort answered in SQL; the count shares `WHERE` with the
      select, and the role predicate is an `EXISTS` so searching one role
      still shows every role a row holds. `directory::list` stays: the REST
      API has four callers of it.

- [x] `phonix-web` The employees grid, first of the four that grow
      The staff list pages server-side: search, both filters and the sort are
      answered in SQL, and the count shares `FROM` and `WHERE` with the select.
      The unpaged `employee::list` is gone at all three layers; the manager
      picker's `employed` is untouched.
