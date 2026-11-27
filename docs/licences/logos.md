# Logos in Settings

A working note, not legal advice.

Settings shows a logo on the card of each service and assistant SoftPresenter connects with
(Settings › Integrations, Settings › AI assistants), so the operator can tell them apart at a
glance. Where there is no logo, the card shows the service's initials and its name.

**The logos of other companies kept here are trademarks of their owners, used only to identify
the apps SoftPresenter connects with; they are not covered by the GPL, and they are removed on
request.** Their permission, where there is one, can't be passed on, so a fork that isn't allowed
to use them leaves them out. They are kept on their own in
[`assets/logos/third-party`](../../assets/logos/third-party), so they are easy to leave out:
`crates/sp-app/src/logos.rs` is the one place that reads them, and a card shows the initials
instead when one is taken away.

## The owner's own

| Logo | Where | Note |
|---|---|---|
| VoxLive | Drawn by `logos.rs` from VoxLive's own mark (three waves from a gold dot) | The owner's own product |
| TimelyChurch | [`assets/logos/timelychurch.png`](../../assets/logos/timelychurch.png) | The owner's own product |

## Other companies' logos

Each is kept exactly as its company provides it (the SVG or PNG as downloaded on 25 November
2026). For an SVG, the window reads a PNG drawn from it at 96 × 96, unaltered in shape and
colour, with `rsvg-convert -w 96 -h 96 NAME.svg -o NAME.png` (librsvg 2.62). The window shows
each at two thirds of its tile, never recoloured or dimmed, and never alone as a button (every
card's key has words).

| Logo | Files | Source | Terms |
|---|---|---|---|
| Grok (Settings › AI assistants) | `grok.svg`, `grok.png` | Grok_Logomark_Light.svg from xAI's assets, data.x.ai/logos/SpaceXAI_Grok_Assets.zip, under xAI's brand guidelines at x.ai/legal/brand-guidelines (as the design session read them on 25 November 2026 and recorded in its handoff/logos.md) | Used only to refer to Grok accurately, exactly as provided, without any alteration, and never so it suggests xAI endorses SoftPresenter; xAI can end the permission at any time |
| Canva (for Canva Sync) | `canva.svg`, `canva.png` | "Canva Icon logo.svg" from canva.dev/assets/connect/Canva-logos.zip, under canva.dev's REST API brand guidelines (as the design session read them on 25 November 2026 and recorded in its handoff/logos.md) | Canva Sync is a Connect API integration: under 50 px the Icon logo is used, with 8 px clear on every side (a 32-pixel icon in a 48-pixel tile), never recoloured, never beside another logo as one mark; the words say "works with Canva", never "by Canva" |
| Claude (Settings › AI assistants) | `claude-spark.svg`, `claude.png` | "Claude Spark - Clay.svg" from Anthropic's press kit, <https://www.anthropic.com/press-kit> (the archive it leads to, <https://assets.claude.com/ae59ca4ca194dac9c9dc3bc78c5829468cb0e8af.zip>; only that one file was taken out of it) | **Used without the owner's approval.** Anthropic's trademark guidelines (<https://www.anthropic.com/legal/trademark-guidelines>) allow its marks only in materials it approves beforehand; the owner of SoftPresenter chose to show it until Anthropic says otherwise |
| ChatGPT (Settings › AI assistants) | `openai-blossom-white.svg`, `chatgpt.png` | "OAI_OpenAI-Blossom_White.svg" from OpenAI's logo kit, <https://cdn.openai.com/brand/openai-logos.zip> (the download the design session's record gives for openai.com/brand; only that one file was taken out of it); OpenAI's brand page offers no ChatGPT logo of its own | **Used without the owner's approval.** OpenAI's brand page allows the logo only where it relates directly to OpenAI's services, exactly as provided, acknowledged as OpenAI's, never recoloured, and also asks for permission; the owner of SoftPresenter chose to show it until OpenAI says otherwise |
| NDI (Settings › Integrations) | `ndi.svg`, `ndi.png` | "NDI Logo Master - Light.svg" from "NDI Logo (Docs & Guides).zip", the logo package NDI offers on its brand pages, docs.ndi.video/brand/brand-elements/logo (as the design session read them on 28 November 2026 and recorded in its handoff/logos.md). The PNG is drawn from it 384 wide (`rsvg-convert -w 384 ndi.svg -o ndi.png`) | NDI's brand pages: the current logo only; the light logo on dark grounds; keep its safety margins (they are in the file, and the tile shows the whole file across its width); keep contrast; never alter it. NDI's SDK licence (clause 3f) allows NDI's trademarks only "to identify that the Product is compatible with NDI Products", with "special and clear notations … that the marks are NDI trademarks" (the card carries the trademark line), and nothing may "suggest sponsorship by NDI" |
| Home Assistant (kept ready; no card shows it until SoftPresenter connects to Home Assistant) | `home-assistant.svg`, `home-assistant.png` | `home-assistant/logo/screen/logomark/HA-logomark-color.svg` from the Open Home Foundation's brand assets, github.com/OpenHomeFoundation/brand-assets, under contributors.home-assistant.io/brand/name-and-logo (as the design session read them on 28 November 2026 and recorded in its handoff/logos.md). The PNG is drawn from it at 96 × 96 | A trademark of the Open Home Foundation; commercial use needs their written permission ("not available for commercial use without express written permission", commercial being "anything designed to market or promote a product, software or service that is for sale"). SoftPresenter is free, so this is our reading that it's allowed; ask partner@openhomefoundation.org first if it's ever sold. For whoever builds the card: the logo sits in the same rounded tile as every other card's, at the usual two thirds of the tile. **That is the owner's decision** (28 November 2026: every integration card has the rounded box round its logo, and this one should too), against the foundation's guide, which says not to put the logomark in a square, a coloured shape or any other container; it is changed back on his request or the foundation's. Kept from the guide: the colour version on the dark ground; no shadow, no outline; clear space of at least a quarter of its height; the name always "Home Assistant" in full, never "HA", and no initials; no "Works with Home Assistant" claim or badge (that programme is for certified device makers) |
| Worship Leader App (Settings › Integrations) | `worship-leader-app.png` | The logo in the header of its own site, <https://worshipleaderapp.com/images/worshipleader-note-white.png> (160 × 160) | **Used without the owner's approval.** Worship Leader App publishes no terms for its logo |

Gemini's icon is not here: Google offers its product icons only through an approval form behind
a sign-in (its Partner Marketing Hub), so the Gemini tile shows its initials.

[NOTICE.md](../../NOTICE.md) names them.
