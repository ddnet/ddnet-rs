use super::*;
use game_interface::votes::*;
use game_network::messages::{MsgSvLoadVotes, MsgSvResetVotes};

pub(super) fn load(client: &mut Client, votes: MsgSvLoadVotes) {
    match votes {
        MsgSvLoadVotes::Map { categories, .. } => {
            for (category, maps) in categories {
                for (map, _) in maps {
                    let label = format!("{}: {}", category.as_str(), map.name.as_str());
                    insert(
                        client,
                        label,
                        VoteIdentifierType::Map(MapCategoryVoteKey {
                            category: category.clone(),
                            map,
                        }),
                    );
                }
            }
        }
        MsgSvLoadVotes::Misc { votes: categories } => {
            for (category, votes) in categories {
                for (vote_key, _) in votes {
                    let label =
                        format!("{}: {}", category.as_str(), vote_key.display_name.as_str());
                    insert(
                        client,
                        label,
                        VoteIdentifierType::Misc(MiscVoteCategoryKey {
                            category: category.clone(),
                            vote_key,
                        }),
                    );
                }
            }
        }
    }
}

fn insert(client: &mut Client, label: String, vote: VoteIdentifierType) {
    if client.votes.len() >= 4096 {
        return;
    }
    let label = format!(
        "{} {}",
        client.votes.len() + 1,
        text::<48>(label.as_bytes()).as_str()
    );
    client.votes.insert(label.clone(), vote);
    client.vote_queue.push_back(label);
}

pub(super) fn reset(client: &mut Client, kind: MsgSvResetVotes) {
    client.votes.retain(|_, v| {
        !matches!(
            (kind, v),
            (MsgSvResetVotes::Map, VoteIdentifierType::Map(_))
                | (MsgSvResetVotes::Misc, VoteIdentifierType::Misc(_))
        )
    });
    client.vote_queue = client.votes.keys().cloned().collect();
}

pub(super) fn state(
    net: &mut Net<SocketAddr>,
    socket: &mut Socket,
    pid: PeerId,
    vote: Option<VoteState>,
) -> anyhow::Result<()> {
    let Some(vote) = vote else {
        return send_game(
            net,
            socket,
            pid,
            game::SvVoteSet {
                timeout: 0,
                description: b"",
                reason: b"",
            },
        );
    };
    let (description, reason) = match &vote.vote {
        VoteType::Map { key, .. } => (format!("Map: {}", key.map.name.as_str()), String::new()),
        VoteType::Misc { key, .. } => {
            (key.vote_key.display_name.as_str().to_owned(), String::new())
        }
        VoteType::VoteKickPlayer { name, key, .. } => (
            format!("Kick {}", name.as_str()),
            key.reason.as_str().to_owned(),
        ),
        VoteType::VoteSpecPlayer { name, key, .. } => (
            format!("Spectate {}", name.as_str()),
            key.reason.as_str().to_owned(),
        ),
        VoteType::RandomUnfinishedMap { .. } => ("Random unfinished map".into(), String::new()),
    };
    let description = text::<64>(description.as_bytes());
    let reason = text::<64>(reason.as_bytes());
    send_game_ref(
        net,
        socket,
        pid,
        game::SvVoteSet {
            timeout: vote.remaining_time.as_secs().clamp(1, 60) as i32,
            description: description.as_str().as_bytes(),
            reason: reason.as_str().as_bytes(),
        }
        .into(),
    )?;
    let total = vote.allowed_to_vote_count.min(64) as i32;
    let yes = vote.yes_votes.min(64) as i32;
    let no = vote.no_votes.min(64) as i32;
    send_game(
        net,
        socket,
        pid,
        game::SvVoteStatus {
            yes,
            no,
            pass: (total - yes - no).max(0),
            total,
        },
    )
}
