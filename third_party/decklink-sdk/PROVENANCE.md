# DeckLink SDK headers

These are header files from Blackmagic Design's DeckLink SDK, version 16.0, copied unchanged
from the SDK's `Mac/include` and `Linux/include` folders. SoftPresenter's bridge to the DeckLink
driver (`crates/sp-decklink/bridge/bridge.cpp`) is compiled with them, on a Mac with the ones
in `Mac/include` and on Linux with the ones in `Linux/include`. Only the headers that bridge
needs are here; no other file from the SDK (no sample code, no documentation, no
`DeckLinkAPIDispatch.cpp`) is in this repository.

Each file keeps Blackmagic Design's copyright notice and licence statement at its top, as that
statement requires. The SDK's End User License Agreement (clause 0.1) does not apply its
clauses 1, 4.3, 4.4, 5, 7 and 8 to the files in these folders, so they may be copied and passed
on under the statement in each file. [`docs/licences/decklink.md`](../../docs/licences/decklink.md)
explains.

Nothing here is the DeckLink driver itself (Desktop Video), which the person using SoftPresenter
installs.

## The files, and their SHA-256 checksums as copied

    ec4e03ccc7885a855f9d5b62a6832509da34cd55ac9a11ab4fbcc72abdedbd77  Mac/include/DeckLinkAPI.h
    e9ac1108e4fa7c738ea147eebaedb603b21cdc0b3f8ce308cec13b4b9adfe88a  Mac/include/DeckLinkAPIConfiguration.h
    314fc4c935932086daaa8059dc490349a7b36656a3077580c27128fb30bb18f8  Mac/include/DeckLinkAPIDeckControl.h
    b95e2c3bd01f1360c29695b3d6e68b2bb3f644d90ddbf81e86ef06bd0e7e7327  Mac/include/DeckLinkAPIDiscovery.h
    8a3e453553df384e70404a91d811ac0588d0a1894d2203e261b5dbae38d27865  Mac/include/DeckLinkAPIModes.h
    ea2624b56f39c2ae5104aa87a580cce190833a37302301f0a40cd9c5709858da  Mac/include/DeckLinkAPIStreaming.h
    b0ada762c45a9f09f4e1333f5c9e2e5fc8f38d563fcc5e54df6d0d375aed9988  Mac/include/DeckLinkAPITypes.h
    074dd64520cf26eb58334390fef661aef503af584244be50846321fc905ba4f4  Mac/include/DeckLinkAPIVersion.h
    96685d2cd2fd36429c1e41388ae54c4cf18572a2d86002beccb38afe7306e2d9  Linux/include/DeckLinkAPI.h
    bdab9934f1c9d4026251d3cafd65cc3422015afd4b086c54ab35909137ed9392  Linux/include/DeckLinkAPIConfiguration.h
    9af52a6241100c7d354c930e0f787571886e8ec2d03b0ada64d6f35aa4797e78  Linux/include/DeckLinkAPIDeckControl.h
    590c9fa75c247d62dd3bd554a27721e95503562aae094a7600ed916465ef9432  Linux/include/DeckLinkAPIDiscovery.h
    73a0b682d177bf88a83ccfee27e3715ec532326b5f2474112686ea6d964a2f44  Linux/include/DeckLinkAPIModes.h
    d981b2db74158a083af8d205054e55ef9fb50aa969d6c9dc92912f4514d19502  Linux/include/DeckLinkAPITypes.h
    074dd64520cf26eb58334390fef661aef503af584244be50846321fc905ba4f4  Linux/include/DeckLinkAPIVersion.h
    0deb38ada25ef1dcfbde4839e72dd59b6a3e91d3e58199c6679b2f746f2f3bef  Linux/include/LinuxCOM.h

To check them, run `shasum -a 256 Mac/include/* Linux/include/*` in this folder and compare
with the list above. To move to a
newer SDK, copy the same files from its `Mac/include` and `Linux/include` folders unchanged,
read their notices, update the version and the checksums here, and check the entry points the
bridge looks up against the new SDK's `DeckLinkAPIDispatch.cpp`.
