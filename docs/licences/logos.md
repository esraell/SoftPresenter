# Logos in Settings

A working note, not legal advice.

Settings shows a logo on the card of each service and assistant SoftPresenter connects with
(Settings › Integrations, Settings › AI assistants), so the operator can tell them apart at a
glance. Where there is no logo, the card shows the service's initials and its name.

**The GNU GPL does not cover the logos of other companies kept here.** They are trademarks of
their owners, used only to identify the apps SoftPresenter connects with; their permission can't
be passed on, so a fork that isn't allowed to use them leaves them out. They are removed on
request. They are kept on their own in [`assets/logos/third-party`](../../assets/logos/third-party),
so they are easy to leave out: `crates/sp-app/src/logos.rs` is the one place that reads them,
and a card shows the initials instead when one is taken away.

## The owner's own

| Logo | Where | Note |
|---|---|---|
| VoxLive | Drawn by `logos.rs` from VoxLive's own mark (three waves from a gold dot) | The owner's own product |
| TimelyChurch | [`assets/logos/timelychurch.png`](../../assets/logos/timelychurch.png) | The owner's own product |

## Other companies' logos

Each is kept exactly as its company provides it (the SVG as downloaded), with a PNG drawn from
it at 96 × 96 for the window, unaltered in shape and colour, with
`rsvg-convert -w 96 -h 96 NAME.svg -o NAME.png` (librsvg 2.62). The window shows it at two
thirds of its tile, never recoloured or dimmed, never alone as a button (every card's key has
words).

| Logo | File | Source, read on 25 November 2026 | The rule followed |
|---|---|---|---|
| Canva (for Canva Sync) | `canva.svg`, `canva.png` | "Canva Icon logo.svg" from <https://www.canva.dev/assets/connect/Canva-logos.zip>, under the REST API brand guidelines at <https://www.canva.dev/docs/connect/guidelines/brand/> | Canva Sync is a Connect API integration: under 50 px the Icon logo is used, with 8 px clear on every side (a 32-pixel icon in a 48-pixel tile), never recoloured, never beside another logo as one mark; the words say "works with Canva", never "by Canva" |

[NOTICE.md](../../NOTICE.md) names them.
