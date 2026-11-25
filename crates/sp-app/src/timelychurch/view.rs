//! What Settings › Integrations › TimelyChurch shows (a copy of the connection's state, made
//! again only when it changes) and asks for. The pairing code typed is never kept here past
//! Connect.

use super::sync::Status;

/// The card's state and what it asks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    /// Offered in this build (the testing switch is set); otherwise there is no card.
    pub offered: bool,
    /// TimelyChurch's page is open (otherwise the list of services).
    pub open: bool,
    /// The card asks for the church's address (TimelyChurch may turn out to have one address
    /// for every church: then only the pairing code).
    pub asks_address: bool,
    pub typed_address: String,
    pub typed_code: String,
    pub status: Status,
    pub church: String,
    pub address: String,
    /// When TimelyChurch last answered a check (seconds since 1970).
    pub last_checked: Option<i64>,
    /// A check is under way.
    pub checking: bool,
    /// The last problem, in a sentence.
    pub trouble: Option<String>,
    /// Why the last Connect wasn't started (an address that isn't one, no code).
    pub problem: Option<String>,
    /// The playlists made from TimelyChurch's services, soonest first.
    pub services: Vec<ServiceRow>,
    /// Songs shown in services that TimelyChurch hasn't been told of yet.
    pub uses_waiting: usize,
    pub request: Option<Request>,
}

/// A playlist made from a TimelyChurch service, as the card lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceRow {
    pub name: String,
    pub date: Option<String>,
    pub songs: usize,
    pub state: RowState,
}

/// How a playlist stands against its service in TimelyChurch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowState {
    /// As TimelyChurch has it.
    InStep,
    /// Changed here, and TimelyChurch has nothing newer.
    ChangedHere,
    /// Changed here, and TimelyChurch has changed it too: its version is offered, not forced.
    NewerWaiting,
    /// TimelyChurch no longer has the service; the playlist stays.
    Removed,
}

/// What the card asks of the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Pair with the church at this address, with this pairing code.
    Connect {
        address: String,
        code: String,
    },
    CheckNow,
    /// Forget the token here; the songs and playlists made stay.
    Disconnect,
}
