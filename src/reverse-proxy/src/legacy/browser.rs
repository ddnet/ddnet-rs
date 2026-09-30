use super::*;
use game_base::s2s::ServerInfo;
use libtw2_gamenet_ddnet::msg::{ClientsData, Connless, connless};

pub(super) fn respond(
    net: &mut Net<SocketAddr>,
    socket: &mut Socket,
    addr: SocketAddr,
    data: &[u8],
    info: &ServerInfo,
) -> anyhow::Result<()> {
    let Ok(Connless::RequestInfo(request)) =
        Connless::decode(&mut Ignore, &mut Unpacker::new(data))
    else {
        return Ok(());
    };
    let info = &info.browser_info;
    // The original getinfo response has room for 16 clients.
    let mut clients = ArrayVec::<[u8; 2048]>::new();
    for player in info.players.iter().take(16) {
        with_packer(&mut clients, |p| {
            connless::Client {
                name: player.name.as_str().as_bytes(),
                clan: player.clan.as_str().as_bytes(),
                country: -1,
                score: player.score.as_str().parse().unwrap_or(0),
                is_player: 1,
            }
            .encode(p)
        })
        .map_err(|e| anyhow::anyhow!("browser client: {e:?}"))?;
    }
    let msg = Connless::Info(connless::Info {
        token: request.token as i32,
        version: b"0.6.4, legacy-server-proxy",
        name: info.name.as_str().as_bytes(),
        map: info.map.name.as_str().as_bytes(),
        game_type: info.game_type.as_str().as_bytes(),
        flags: i32::from(info.passworded),
        num_players: info.players.len().min(16) as i32,
        max_players: info.max_ingame_players.min(16) as i32,
        num_clients: info.players.len().min(16) as i32,
        max_clients: info.max_players.min(16) as i32,
        clients: ClientsData::from_bytes(&clients),
    });
    let mut buf = ArrayVec::<[u8; 2048]>::new();
    with_packer(&mut buf, |p| msg.encode(p)).map_err(|e| anyhow::anyhow!("browser info: {e:?}"))?;
    net.send_connless(socket, addr, &buf)
        .map_err(|e| anyhow::anyhow!("browser send: {e:?}"))?;
    Ok(())
}

/// Legacy HTTP master-server schema. All protocol-specific fields stay here.
pub(super) fn registration(info: &ServerInfo) -> anyhow::Result<String> {
    let info = &info.browser_info;
    let clients=info.players.iter().map(|p|serde_json::json!({"name":p.name.as_str(),"clan":p.clan.as_str(),"country":-1,"score":p.score.as_str().parse::<i32>().unwrap_or(0),"is_player":true})).collect::<Vec<_>>();
    Ok(serde_json::to_string(
        &serde_json::json!({"name":info.name.as_str(),"game_type":info.game_type.as_str(),"version":"0.6.4, legacy-server-proxy","map":{"name":info.map.name.as_str()},"passworded":info.passworded,"max_clients":info.max_players.min(64),"max_players":info.max_ingame_players.min(64),"num_clients":clients.len(),"num_players":clients.len(),"clients":clients}),
    )?)
}
