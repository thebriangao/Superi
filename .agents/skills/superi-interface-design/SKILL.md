---
name: superi-interface-design
description: Apply Superi's locked interface appearance and shell design. Use for every Superi UI research, planning, implementation, review, capture, typography, color, spacing, motion, icon, workspace, panel, menu, dialog, viewer, timeline, graph, mixer, scope, or accessibility task.
---

# Superi Interface Design

## Authority

Read this file completely before any Superi interface work.

Treat these decisions as locked product direction. Apply them instead of reopening the design,
substituting a generic design system, or copying another editor. A live checkpoint defines scope and
behavior, but it does not silently revise this design. Change a locked decision only after explicit
user direction.

Use this skill as the authority for what Superi looks and feels like. Use `superi-visual-design` for
research, composition, and iteration procedure. Use `superi-icon-system` for icon production. Use
the construction, capture, testing, integration, and acceptance skills for their respective work.

## Character

Make Superi:

- dark
- vibrant where color has meaning
- spacious between groups
- compact within related groups
- modern
- minimalistic
- selectively Liquid Glass
- complete enough for a professional editor

Do not interpret minimalism as missing controls, hidden capability, giant empty regions, or a
stripped-down website. Preserve professional density while making hierarchy and grouping obvious.

## Materials and surfaces

Use a near-black application back plane. Layer opaque dark charcoal through medium-dark gray
surfaces above it.

Use neighboring dark shades to distinguish regions, not to simulate decorative depth. Join docked
panels into one continuous workspace with aligned seams, subtle line breaks, and deliberate padding.
Do not turn panels into floating cards.

Keep these primary editing surfaces opaque:

- viewers and media canvases
- timelines
- node graphs
- scopes
- mixers
- media grids and browsers
- inspectors and parameter panels
- docked workspace panels

Reserve Liquid Glass for selective transient or elevated surfaces:

- the global top command area when translucency remains legible
- viewer HUDs and floating transport controls
- menus, context menus, popovers, dropdowns, and tooltips
- command palette and global search
- modal sheets
- temporary quick inspectors

Never spread glass across the whole editor. Keep the media and primary task surface visually stable.

## Color

Do not invent a permanent brand accent color.

Use pure white and a restrained range of light grays for neutral hierarchy. Use a light-gray filled
highlight for selected rows, items, and choices. Ensure selected foreground content remains
readable against that fill.

Use semantic color only when it communicates function, state, category, risk, signal, or useful
distinction. Give every permanent semantic color one stable meaning. Avoid decorative rainbow
treatment and avoid assigning color to every control.

Allow each workspace to own a restrained theme color for its icon and active underline. Choose that
color from the workspace's actual role, check it against existing semantic meanings, and keep it
stable once registered.

Keep focus, selection, hover, active state, warning, error, recording, and disabled state distinct.
Do not rely on color alone for required meaning.

## Typography

Bundle Supreme from Fontshare as a required product asset. Do not silently substitute another
interface font in production or acceptance captures.

Use these weights:

- Regular for body text
- Medium for controls and ordinary labels
- Semibold for headers and active labels

Do not use Bold.

Use only these interface text sizes:

- 16 px for the largest interface identities, including the project title and Superi identity
- 14 px for primary panel and section headers
- 13 px for ordinary controls and body text
- 12 px for secondary labels and metadata
- 11 px for dense technical information
- 10 px for the smallest professional-editor details

Do not use 15 px. Do not shrink interface text below 10 px.

Use Title Case for interface labels, navigation, controls, and headings. Preserve natural casing for
filenames, user content, paths, timecode, metadata, prose messages, and technical values. Use
tabular numerals where changing number widths would disturb alignment.

## Density and spacing

Keep individual controls visible and medium-sized rather than tiny or oversized.

Place strongly related controls close together with comfortable breathing room. Give unrelated
groups noticeably more separation. Let proximity explain functional grouping. Do not apply one
large uniform gap to every item, and do not scatter buttons inches apart in the name of spaciousness.

Keep rows inside one control group compact enough for fast repeated work. Separate the group itself
generously from viewers, timelines, graphs, and unrelated categories.

## Shape and state

Use medium-soft control corners. Make them more rounded than traditional rigid professional
software, but less rounded than a contemporary consumer website. Avoid turning every control into a
pill.

Use the light-gray filled selection treatment consistently. Keep hover quieter than selection.
Keep keyboard focus independently visible. Make pressed, toggled, disabled, busy, warning, and
destructive states explicit.

## Motion

Use fluid, water-smooth motion often enough that the editor feels alive, without turning the
interface into a liquid simulation.

Choose different motion curves and durations for different actions. Allow the complete useful range
from near-instant feedback to longer continuity-preserving transitions. Keep repeated editing
gestures immediate. Use longer motion only when it explains spatial change, panel movement, or a
meaningful state transition.

Follow an existing local motion standard when one exists. Define and reuse an appropriate standard
when a new motion category is introduced. Do not force every interaction through one global
duration or curve. Provide a reduced-motion equivalent.

## Application shell

Use one main window. Do not create detached panel windows or separate multi-monitor workspace
windows.

Open Superi into a dedicated project chooser page before entering the editor. Allow exactly one
project to be open at a time. Restore that project's last active workspace when it reopens.

Keep a global top bar and bottom workspace bar visible in every editor workspace. Place the current
project name at the top center. Place the permanent Superi identity and future logo at the bottom
left.

Run workspace navigation across the bottom. Always show both icon and text for every workspace,
including constrained layouts. Indicate the active workspace with:

- a darker background segment
- its colored icon
- an underline matching its registered workspace theme color

Keep these permanent workspaces in this order:

1. Organize
2. Source
3. Edit
4. Composite
5. Color
6. Audio
7. Deliver
8. Review

Keep automation, AI management, performance, preferences, and diagnostics out of the permanent
workspace bar. Present them through contextual tools, panels, menus, or secondary surfaces.

Give every workspace a purpose-built layout and a separately organized implementation. Keep the
visual language shared, but do not force all workspaces through one generic panel composition.

Center every workspace on one dominant working surface appropriate to its job. Arrange supporting
panels around that surface. Keep panel positions purpose-built and non-rearrangeable. Enable
resizing, collapsing, or hiding only for panels whose role benefits from that behavior. Do not
assume every panel supports all three.

Do not add a universal left tool rail. Let each workspace own and place its relevant tools.

Dock a collapsible AI Agent on the right side of the editor. Treat Cursor as a behavioral reference
for its role and collapsible placement, not as visual artwork to copy. Keep the Agent available
across workspaces without making it the primary editing surface.

## Icons

Keep Superi icons original, detailed, primarily outline-based, and legible at small sizes. Add
purposeful shading for dimension without approaching photorealism. Use rounded modern forms while
avoiding generic simple-pack, pill, or symbol-library appearance.

Never manually construct a new icon from invented vector geometry.

For every new icon:

1. Define its exact semantic meaning.
2. Search the registry for an existing semantic match.
3. Generate exactly two original starting concepts with GPT Image 2 on a black background, using a
   white icon unless its declared color contract requires color.
4. Inspect both concepts visually at useful crops and actual interface scale.
5. Reject weak, derivative, ambiguous, blob-like, or small-size failures.
6. Generate exactly one revised concept after each rejection.
7. Repeat inspection and revision until the silhouette is excellent.
8. Remove the black background and background-transition pixels while preserving intentional icon
   shading and color.
9. Vectorize the accepted raster without redesigning it from hand-authored geometry.
10. Normalize it to the shared 24 by 24 icon system.
11. Apply optical corrections at actual interface sizes.
12. Test every declared state against Superi's black interface.
13. Register, version, hash, atlas-pack, and integrate it.

Record the semantic brief, initial concepts, rejected concepts, accepted raster, cleaned raster,
vector source, optical adjustments, state contract, version, hash, and capture evidence.

Give at least 10 percent of the complete icon registry a meaningful colored version. Treat this as a
minimum, not a target or cap. Use color in default, hover, active, clicked, toggled, category, or
status states only when it improves recognition, differentiates similar actions, communicates
state, or adds deliberate character.

Tune a grayscale form for every colored icon. Allow important active icons to retain color. Allow
an inactive hovered icon and the active selected icon to show color simultaneously. Keep ordinary
inactive icons grayscale unless their registered default state has a meaningful permanent color.

Never copy, trace, recolor, or derive icon artwork from another product. Reference screenshots
establish behavior and state use only.

## Accessibility and proof

Keep selection distinct from focus. Provide non-color evidence for every required state. Maintain at
least a 24 by 24 logical hit area or equivalent spacing for compact targets. Give visible keyboard
focus a strong 2 px perimeter-equivalent appearance and at least 3:1 change contrast.

Verify compact, default, large-text, high-contrast, and reduced-motion behavior. Inspect complete
surfaces and useful crops through Superi's private retained renderer. Judge typography, hierarchy,
group spacing, seams, panel distinction, glass restraint, icons, state, and media dominance from
real pixels.

Reject any result that feels like a generic website, a stripped-down editor, a card dashboard, an
uncontrolled rainbow, a copied professional editor, or a collection of unrelated components.
