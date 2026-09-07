/**
 * Compile Global Connect's two assets: its stylesheet and its one script.
 *
 *     npm install
 *     node tools/build-site-assets.mjs
 *
 * The same arrangement as `build-desk-assets.mjs`, and for the same reasons -
 * read that file's header for the long version. The short one: this runs by
 * hand, the output is committed, and building and deploying Phonix needs cargo
 * and nothing else. Node is required only to *change* how the site looks.
 *
 * The argument is if anything stronger here than for Desk. Desk is an internal
 * tool; this is the public face, and a release step that can fail on a missing
 * npm is a release step that can leave the front page serving last month's
 * stylesheet.
 *
 * The script is copied rather than bundled: `site.js` has no imports, so there
 * is nothing to bundle and nothing to transpile. A build step that could fail
 * is a strange thing to put in front of an enhancement nobody needs.
 */

import { createHash } from "node:crypto";
import { gzipSync } from "node:zlib";
import { execFileSync } from "node:child_process";
import { readdirSync, mkdirSync, rmSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { collectArtifacts, collectFont, fontFaceCss } from "./site-artifacts.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const crate = join(root, "crates", "global-connect");
const entry = join(crate, "style", "site.css");
const outDir = join(crate, "assets");
const constantFile = join(crate, "src", "assets.rs");
const scratch = join(outDir, ".site.build.css");
const fontFaceFile = join(crate, "style", "_generated-fonts.css");

mkdirSync(outDir, { recursive: true });

// The font first: its `@font-face` carries a content hash, and Tailwind is
// about to read the file that holds it.
const font = collectFont(crate, publish);
writeFileSync(fontFaceFile, fontFaceCss(font).replace(/\r\n/g, "\n"));

// The CLI rather than the JS API, because it is the same program cargo-leptos
// downloads for the product - so all three applications are compiled by the
// same tool reading the same `theme.css`, and a Tailwind release that changes
// how a utility is emitted changes all of them or none.
//
// Its entry point under `node`, rather than the shim in `node_modules/.bin`.
// npm writes three shims per binary and neither of the two that work on Windows
// can be spawned directly: the extensionless one is a shell script, and Node
// has refused to `execFileSync` a `.cmd` without a shell since the argument
// injection fix in 18.20. Running the `.mjs` the shim would have run avoids
// both, and avoids a shell's quoting rules on paths that contain the repository
// root - which is somebody's home directory and may well have a space in it.
const tailwind = join(root, "node_modules", "@tailwindcss", "cli", "dist", "index.mjs");

execFileSync(
    process.execPath,
    [tailwind, "--input", entry, "--output", scratch, "--minify"],
    { cwd: root, stdio: ["ignore", "ignore", "inherit"] },
);

const contents = readFileSync(scratch);
rmSync(scratch);

/**
 * Hash one asset into `assets/`, sweeping away whatever the last build left.
 * Left alone those accumulate one file per edit, none of them compiled into the
 * binary and all of them committed by habit.
 */
/** Every name this run wrote, so the sweep at the end knows what to keep. */
const published = new Set();

function publish(bytes, extension, stem = "site") {
    const hash = createHash("sha256").update(bytes).digest("hex").slice(0, 12);
    const name = `${stem}.${hash}.${extension}`;

    writeFileSync(join(outDir, name), bytes);
    published.add(name);

    return {
        name,
        // Level 9 because the file is written once and read many times.
        gzipped: gzipSync(bytes, { level: 9 }).length,
        bytes: bytes.length,
    };
}

/**
 * Delete everything in `assets/` this run did not write.
 *
 * By the output rather than by the name, and that distinction has already
 * mattered once. A per-stem sweep can only clear a previous version of the
 * *same* asset; delete a screenshot from `artifacts/` and its published copy
 * stays in `assets/` for ever - compiled into nothing, referenced by nothing,
 * and committed by habit.
 *
 * Safe to run last because `assets/` holds nothing that is not generated: every
 * file in it was written by this script, and the two `.rs` files that name them
 * are written afterwards.
 */
function sweep() {
    // `name.<12 hex>.ext` is the shape this script writes. Anything else in
    // here was put there by hand.
    const generated = /^.+\.[0-9a-f]{12}\.[a-z0-9]+$/;

    for (const existing of readdirSync(outDir)) {
        if (published.has(existing)) {
            continue;
        }

        rmSync(join(outDir, existing));

        if (generated.test(existing)) {
            console.log(`  removed ${existing}`);
        } else {
            // Deleting it is right - assets/ is output - but somebody who put a
            // screenshot here meant to add one, and a bare "removed" teaches
            // them nothing about where it should have gone.
            console.log(
                `  removed ${existing} - assets/ is generated output and is swept ` +
                `on every build.
` +
                `    A screenshot belongs in crates/global-connect/artifacts/ ` +
                `(see its README).`,
            );
        }
    }
}

const css = publish(contents, "css");
const js = publish(readFileSync(join(crate, "script", "site.js")), "js");
const { shots, social } = collectArtifacts(crate, publish);

/** A Rust string literal. The names come off the filesystem, so a quote in one
 * would otherwise write a source file that does not compile. */
const rust = (value) => JSON.stringify(String(value));

const shotRows = shots
    .map(
        (shot) =>
            `    Shot {\n` +
            `        app: ${rust(shot.app)},\n` +
            `        screen: ${rust(shot.screen)},\n` +
            `        url: ${rust("/assets/" + shot.name)},\n` +
            `        bytes: include_bytes!(${rust("../assets/" + shot.name)}),\n` +
            `        mime: ${rust(shot.name.endsWith(".png") ? "image/png" : "image/webp")},\n` +
            `        width: ${shot.width},\n` +
            `        height: ${shot.height},\n` +
            `    },`,
    )
    .join("\n");

const socialRust = social
    ? `Some(Shot {
    app: "",
    screen: "og",
    url: ${rust("/assets/" + social.name)},
    bytes: include_bytes!(${rust("../assets/" + social.name)}),
    mime: ${rust(social.name.endsWith(".png") ? "image/png" : "image/webp")},
    width: ${social.width},
    height: ${social.height},
})`
    : "None";

const artifacts = `//! The screenshots, and the card a shared link unfurls into.
//!
//! Generated by \`node tools/build-site-assets.mjs\` from
//! \`crates/global-connect/artifacts\`. Do not edit. The naming rule that put
//! each row here is in that folder's README, and the build refuses a file that
//! breaks it.
//!
//! Compiled in rather than served from a directory, like the stylesheet and for
//! the same two reasons: the site stays one artefact, and a table of known
//! names has no path to traverse.

/// One picture, and everything a page needs to draw it without reflowing.
pub struct Shot {
    /// Which application it belongs to - the part of the filename before the
    /// first hyphen. Empty for the social card, which belongs to none.
    pub app: &'static str,
    pub screen: &'static str,
    pub url: &'static str,
    pub bytes: &'static [u8],
    pub mime: &'static str,
    /// Its real dimensions, read out of the file at build time. Both reach the
    /// \`<img>\`, because one without the other is a page that reflows when the
    /// picture lands - worst on the slow connection this site is kind to.
    pub width: u32,
    pub height: u32,
}

/// Every screenshot, sorted by filename.
pub static SHOTS: &[Shot] = &[
${shotRows}
];

/// The 1200x630 card, when \`artifacts/og-image\` exists.
pub static SOCIAL: Option<Shot> = ${socialRust};

/// The handwriting face, when one has been dropped into \`fonts/\`.
pub static HANDWRITING: Option<(&str, &[u8])> = ${
    font
        ? `Some((${rust("/assets/" + font.name)}, include_bytes!(${rust("../assets/" + font.name)})))`
        : "None"
};

impl Shot {
    /// Find every shot for one application, in filename order.
    pub fn of(app: &str) -> impl Iterator<Item = &'static Shot> {
        SHOTS.iter().filter(move |shot| shot.app == app)
    }
}
`;

writeFileSync(join(crate, "src", "artifacts.rs"), artifacts.replace(/\r\n/g, "\n"));

// Last, once everything that survives has been written.
sweep();

// `\n` explicitly and no trailing spaces: `.gitattributes` normalises this tree
// to LF, and a generated file that comes back CRLF on a Windows checkout is a
// whole-file diff on every run.
const source = `//! Where the site's assets are, and what is in them.
//!
//! Generated by \`node tools/build-site-assets.mjs\`. Do not edit: the hash in
//! each URL is the hash of the file the script wrote, and changing one without
//! the other serves a page that points at nothing.
//!
//! The files' sizes are not published here, for the reason Desk's equivalent
//! gives: a constant nothing reads is a fact that stops being true quietly. The
//! script prints them when it runs, and that is where the claim in ADR 0007
//! about this site's weight should be checked.

/// The stylesheet's URL.
///
/// Named by [\`base.html\`](../templates/base.html) and answered by
/// [\`crate::routes::stylesheet\`]. The hash means a browser may keep it
/// forever, and that a changed stylesheet is a different address rather than a
/// stale copy.
pub const STYLESHEET: &str = "/assets/${css.name}";

/// The bytes themselves, compiled into the binary.
///
/// One artefact: copy the binary, run it. A static directory beside it is a
/// second thing to deploy and a second thing to get wrong.
pub const STYLESHEET_CSS: &str = include_str!("../assets/${css.name}");

/// The script's URL, hashed and cached the same way.
///
/// Everything it does is optional - see \`script/site.js\`. A page is complete
/// without it, which is what lets it be loaded \`defer\` and never waited on.
pub const SCRIPT: &str = "/assets/${js.name}";

pub const SCRIPT_JS: &str = include_str!("../assets/${js.name}");
`;

writeFileSync(constantFile, source.replace(/\r\n/g, "\n"));

for (const asset of [css, js]) {
    console.log(`  wrote crates/global-connect/assets/${asset.name}`);
    console.log(
        `  ${(asset.bytes / 1024).toFixed(1)} KiB uncompressed, ` +
        `${(asset.gzipped / 1024).toFixed(1)} KiB gzipped`,
    );
}
console.log("  wrote crates/global-connect/src/assets.rs");
console.log("  wrote crates/global-connect/src/artifacts.rs");

// The weight of the pictures, said out loud. It is the one number on this site
// that can grow without anybody deciding to grow it - see ADR 0007 section 3.
const pictures = shots.reduce((total, shot) => total + shot.bytes, 0);
console.log(
    `  ${shots.length} screenshot(s), ${(pictures / 1024).toFixed(1)} KiB in the binary` +
    (social ? ", plus a social card" : "") +
    (font ? ", plus a handwriting face" : ""),
);
