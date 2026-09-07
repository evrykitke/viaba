/**
 * The screenshots, and the optional handwriting font.
 *
 * Imported by `build-site-assets.mjs`; not run on its own. It is a separate
 * file because the stylesheet build is one idea and scanning a folder of
 * pictures is another, and the two share nothing but the hashing.
 *
 * # Why the images are compiled in like everything else
 *
 * ADR 0007 §3 says the site is one artefact: copy the binary, run it. A static
 * directory beside it is a second thing to deploy and a second thing to get
 * wrong, and it is also a path-traversal surface that a table of known names
 * simply does not have.
 *
 * The cost is honest and worth writing down: every screenshot is in the binary,
 * so the binary grows by roughly the weight of this folder. That is why
 * `MAX_BYTES` exists and why it refuses rather than warns.
 */

import { createHash } from "node:crypto";
import { readdirSync, readFileSync, existsSync } from "node:fs";
import { join, extname, basename } from "node:path";

/** Applications a screenshot may belong to.
 *
 * The second copy of this list; the first is `APP_SLUGS` in
 * `src/routes/pages.rs`, and a test there fails if the two disagree. Kept here
 * rather than parsed out of the Rust so that a mistyped filename is caught by
 * the thing that reads the folder, at the moment somebody adds the file. */
const APPS = ["books", "inventory", "people"];

/** A page carrying four of these is already heavier than the whole rest of the
 * site, so this refuses rather than warns. */
const MAX_BYTES = 250 * 1024;

/** Below this a screenshot is soft on a 2x display, which is most of them. */
const MIN_WIDTH = 1600;

/** The card a link to the site unfurls into. Exempt from the naming rule
 * because it belongs to no application - it is the site's own picture. */
const SOCIAL_CARD = "og-image";

/** Width and height, read out of the file itself.
 *
 * Not to be clever: `<img>` without both is a page that reflows when the
 * picture arrives, and the reflow is worst on the slow connection the site is
 * supposed to be kind to.
 */
function dimensions(bytes, name) {
    // PNG: 8-byte signature, then a 4-byte length and "IHDR", then the two.
    if (bytes.length > 24 && bytes.readUInt32BE(0) === 0x89504e47) {
        return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
    }

    if (bytes.length > 30 && bytes.toString("ascii", 0, 4) === "RIFF" &&
        bytes.toString("ascii", 8, 12) === "WEBP") {
        const chunk = bytes.toString("ascii", 12, 16);

        // Lossy. The two 14-bit values sit just past the start code.
        if (chunk === "VP8 ") {
            return {
                width: bytes.readUInt16LE(26) & 0x3fff,
                height: bytes.readUInt16LE(28) & 0x3fff,
            };
        }

        // Lossless. Fourteen bits each, packed across four bytes, minus one.
        if (chunk === "VP8L") {
            const bits = bytes.readUInt32LE(21);
            return {
                width: (bits & 0x3fff) + 1,
                height: ((bits >> 14) & 0x3fff) + 1,
            };
        }

        // Extended - what a file with transparency or animation gets.
        if (chunk === "VP8X") {
            const three = (at) => bytes[at] | (bytes[at + 1] << 8) | (bytes[at + 2] << 16);
            return { width: three(24) + 1, height: three(27) + 1 };
        }
    }

    throw new Error(`${name}: not a PNG or a WebP this can measure`);
}

/**
 * Scan `artifacts/`, returning what the Rust table needs.
 *
 * `publish` is the hashing function from the caller, so both halves of the
 * build name their files the same way.
 */
export function collectArtifacts(crate, publish) {
    const dir = join(crate, "artifacts");
    const shots = [];
    let social = null;

    if (!existsSync(dir)) {
        return { shots, social };
    }

    for (const file of readdirSync(dir).sort()) {
        const ext = extname(file).toLowerCase();
        if (ext !== ".webp" && ext !== ".png") {
            continue;
        }

        const stem = basename(file, ext);
        const bytes = readFileSync(join(dir, file));

        if (bytes.length > MAX_BYTES) {
            throw new Error(
                `${file} is ${(bytes.length / 1024).toFixed(0)} KiB, over the ` +
                `${MAX_BYTES / 1024} KiB ceiling. Every screenshot is compiled into ` +
                `the binary - re-export it smaller, or as WebP if it is a PNG.`,
            );
        }

        const size = dimensions(bytes, file);
        const published = publish(bytes, ext.slice(1), stem);

        if (stem === SOCIAL_CARD) {
            // 1200x630 is what the card readers crop to; anything else is
            // cropped for you, usually through the middle of the wordmark.
            if (size.width !== 1200 || size.height !== 630) {
                throw new Error(
                    `${file} is ${size.width}x${size.height}. The social card must be ` +
                    `1200x630 - every reader crops to that ratio.`,
                );
            }
            social = { ...published, ...size };
            continue;
        }

        if (size.width < MIN_WIDTH) {
            throw new Error(
                `${file} is ${size.width}px wide, under the ${MIN_WIDTH}px minimum. ` +
                `It will be soft on a 2x display.`,
            );
        }

        const dash = stem.indexOf("-");
        const app = dash === -1 ? stem : stem.slice(0, dash);

        if (!APPS.includes(app)) {
            throw new Error(
                `${file} starts with "${app}", which is not an application. ` +
                `Expected one of: ${APPS.join(", ")}. See artifacts/README.md - ` +
                `the name is the wiring, so a typo here is a picture that would ` +
                `otherwise never appear and never be missed.`,
            );
        }

        shots.push({
            app,
            screen: dash === -1 ? "" : stem.slice(dash + 1),
            ...published,
            ...size,
        });
    }

    return { shots, social };
}

/**
 * The handwriting face, if somebody has dropped one in.
 *
 * `fonts/handwriting.woff2`, and nothing else is looked for. Absent is the
 * normal case and is not an error: `.handwritten` falls back to the system
 * cursive stack, which is worse and is not broken.
 *
 * A font is the one asset here that cannot be inline SVG, so it is also the one
 * place the site fetches a third file. That is why it is opt-in.
 */
export function collectFont(crate, publish) {
    const file = join(crate, "fonts", "handwriting.woff2");

    if (!existsSync(file)) {
        return null;
    }

    return publish(readFileSync(file), "woff2", "handwriting");
}

/** The `@font-face` Tailwind reads, written where it can see it.
 *
 * Generated rather than hand-written because the URL carries a content hash,
 * and the stylesheet is compiled before the hash is known to anybody but this
 * script. An absent font writes a comment, so the import in `site.css` is
 * always valid.
 */
export function fontFaceCss(font) {
    if (!font) {
        return [
            "/* Generated by tools/build-site-assets.mjs - do not edit. */",
            "/* No crates/global-connect/fonts/handwriting.woff2, so `.handwritten`",
            "   falls back to the system cursive stack. See artifacts/README.md. */",
            "",
        ].join("\n");
    }

    return [
        "/* Generated by tools/build-site-assets.mjs - do not edit. */",
        "@font-face {",
        '    font-family: "Site Hand";',
        `    src: url("/assets/${font.name}") format("woff2");`,
        "    font-weight: 400 700;",
        "    font-display: swap;",
        "    /* Latin only. Chinese falls through to the CJK stack in `site.css`,",
        "       which is the correct outcome: a Latin hand has no CJK glyphs, and",
        "       per-glyph fallback is what stops a mixed line breaking. */",
        "    unicode-range: U+0000-00FF, U+2000-206F, U+2190-21FF, U+2C60-2C7F;",
        "}",
        "",
    ].join("\n");
}

export const LIMITS = { MAX_BYTES, MIN_WIDTH, APPS, SOCIAL_CARD };
