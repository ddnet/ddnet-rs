use std::collections::BTreeMap;

use base::network_string::NetworkString;
use game_interface::votes::{MAX_CATEGORY_NAME_LEN, MapVote, MapVoteKey, MiscVote, MiscVoteKey};
use hiarc::{Hiarc, hiarc_safer_rc_refcell};
use url::Url;

#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc, Default)]
pub struct Votes {
    map_votes: BTreeMap<NetworkString<MAX_CATEGORY_NAME_LEN>, BTreeMap<MapVoteKey, MapVote>>,
    has_unfinished_map_votes: bool,
    need_map_votes: bool,
    thumbnail_server_resource_download_url: Option<Url>,

    misc_votes: BTreeMap<NetworkString<MAX_CATEGORY_NAME_LEN>, BTreeMap<MiscVoteKey, MiscVote>>,
    need_misc_votes: bool,
}

#[hiarc_safer_rc_refcell]
impl Votes {
    pub fn request_map_votes(&mut self) {
        self.need_map_votes = true;
    }

    /// Automatically resets the "need" state, so
    /// another [`Votes::request_map_votes`] has to
    /// be called.
    pub fn needs_map_votes(&mut self) -> bool {
        std::mem::replace(&mut self.need_map_votes, false)
    }

    pub fn fill_map_votes(
        &mut self,
        map_votes: BTreeMap<NetworkString<MAX_CATEGORY_NAME_LEN>, BTreeMap<MapVoteKey, MapVote>>,
        has_unfinished_map_votes: bool,
    ) {
        self.map_votes = map_votes;
        self.has_unfinished_map_votes = has_unfinished_map_votes;
    }

    pub fn collect_map_votes(
        &self,
    ) -> BTreeMap<NetworkString<MAX_CATEGORY_NAME_LEN>, BTreeMap<MapVoteKey, MapVote>> {
        self.map_votes.clone()
    }

    pub fn has_unfinished_map_votes(&self) -> bool {
        self.has_unfinished_map_votes
    }

    pub fn set_thumbnail_server_resource_download_url(&mut self, url: Option<Url>) {
        self.thumbnail_server_resource_download_url = url;
    }

    pub fn thumbnail_server_resource_download_url(&self) -> Option<Url> {
        self.thumbnail_server_resource_download_url.clone()
    }

    pub fn request_misc_votes(&mut self) {
        self.need_misc_votes = true;
    }

    /// Automatically resets the "need" state, so
    /// another [`Votes::request_misc_votes`] has to
    /// be called.
    pub fn needs_misc_votes(&mut self) -> bool {
        std::mem::replace(&mut self.need_misc_votes, false)
    }

    pub fn fill_misc_votes(
        &mut self,
        misc_votes: BTreeMap<NetworkString<MAX_CATEGORY_NAME_LEN>, BTreeMap<MiscVoteKey, MiscVote>>,
    ) {
        self.misc_votes = misc_votes;
    }

    pub fn collect_misc_votes(
        &self,
    ) -> BTreeMap<NetworkString<MAX_CATEGORY_NAME_LEN>, BTreeMap<MiscVoteKey, MiscVote>> {
        self.misc_votes.clone()
    }
}
