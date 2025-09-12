use std::{collections::BTreeMap, net::SocketAddr, time::Duration};

use anyhow::anyhow;
use arrayvec::ArrayVec;
use base::{hash::Hash, reduced_ascii_str::ReducedAsciiString};
use base_io::io::Io;
use game_interface::types::{
    character_info::NetworkCharacterInfo,
    input::{CharacterInput, CharacterInputMethodFlags},
};
use game_server::client::ServerClientPlayer;
use hexdump::hexdump_iter;
use libtw2_gamenet_ddnet::{
    msg::{Connless, Game, System},
    snap_obj,
};
use libtw2_net::{Net, net::Chunk, net::ChunkOrEvent, net::PeerId};
use libtw2_packer::with_packer;
use libtw2_snapshot::Manager;
use log::{Level, debug, log, log_enabled};

use crate::{ServerInfo, socket::Socket};

pub struct SocketClient {
    pub socket: Socket,
    pub net: Net<SocketAddr>,
    pub server_pid: PeerId,
    pub skip_disconnect_on_drop: bool,

    is_connected: bool,
}

impl SocketClient {
    pub fn new(io: &Io, addr: SocketAddr) -> anyhow::Result<SocketClient> {
        let mut socket = Socket::new(io).unwrap();
        let mut net = Net::client();
        let (server_pid, res) = net.connect(&mut socket, addr);

        res.map_err(|err| anyhow!(err)).map(|_| SocketClient {
            socket,
            net,
            server_pid,
            skip_disconnect_on_drop: false,
            is_connected: false,
        })
    }
    pub fn run_recv(
        &mut self,
        res: (SocketAddr, Vec<u8>),
        on_event: &mut impl FnMut(&mut Self, ChunkOrEvent<'_, SocketAddr>),
    ) {
        let (addr, data) = res;
        let mut buf2 = Vec::with_capacity(4096);
        let (iter, res) = self.net.feed(
            &mut self.socket,
            &mut Warn(addr, &data),
            addr,
            &data,
            &mut buf2,
        );
        res.unwrap();
        for mut chunk in iter {
            if !self.net.is_receive_chunk_still_valid(&mut chunk) {
                continue;
            }
            if let ChunkOrEvent::Connect(pid) = chunk {
                self.is_connected = true;
                self.net.accept(&mut self.socket, pid).unwrap();
            } else {
                if let ChunkOrEvent::Ready(_) = &chunk {
                    self.is_connected = true;
                }
                on_event(self, chunk);
            }
        }
    }
    pub fn run_once(&mut self, mut on_event: impl FnMut(&mut Self, ChunkOrEvent<'_, SocketAddr>)) {
        self.net
            .tick(&mut self.socket)
            .for_each(|e| panic!("{e:?}"));

        while let Ok(res) = self.socket.try_recv() {
            self.run_recv(res, &mut on_event);
        }
    }
    pub fn disconnect(&mut self, reason: &[u8]) {
        self.net
            .disconnect(&mut self.socket, self.server_pid, reason)
            .map_err(|err| err.to_string())
            .expect("disconnecting failed:");
    }
    fn send(&mut self, chunk: Chunk) {
        self.net.send(&mut self.socket, chunk).unwrap();
    }
    fn send_connless(&mut self, addr: SocketAddr, data: &[u8]) {
        self.net
            .send_connless(&mut self.socket, addr, data)
            .unwrap();
    }
    pub fn flush(&mut self) {
        self.net
            .flush(&mut self.socket, self.server_pid)
            .map_err(|err| err.to_string())
            .expect("flushing failed:");
    }

    fn sends_impl(msg: System, vital: bool, socket_client: &mut SocketClient) {
        let mut buf: ArrayVec<[u8; 2048]> = ArrayVec::new();
        with_packer(&mut buf, |p| msg.encode(p).unwrap());
        socket_client.send(Chunk {
            pid: socket_client.server_pid,
            vital,
            data: &buf,
        })
    }
    pub fn sends<'a, S: Into<System<'a>>>(&mut self, msg: S) {
        Self::sends_impl(msg.into(), true, self)
    }
    pub fn sendg<'a, G: Into<Game<'a>>>(&mut self, msg: G) {
        fn inner(msg: Game, socket_client: &mut SocketClient) {
            let mut buf: ArrayVec<[u8; 2048]> = ArrayVec::new();
            with_packer(&mut buf, |p| msg.encode(p).unwrap());
            socket_client.send(Chunk {
                pid: socket_client.server_pid,
                vital: true,
                data: &buf,
            })
        }
        inner(msg.into(), self)
    }
    pub fn sendc<'a, C: Into<Connless<'a>>>(&mut self, addr: SocketAddr, msg: C) {
        fn inner(msg: Connless, addr: SocketAddr, socket_client: &mut SocketClient) {
            let mut buf: ArrayVec<[u8; 2048]> = ArrayVec::new();
            with_packer(&mut buf, |p| msg.encode(p).unwrap());
            socket_client.send_connless(addr, &buf)
        }
        inner(msg.into(), addr, self)
    }
}

impl Drop for SocketClient {
    fn drop(&mut self) {
        if !self.skip_disconnect_on_drop && self.is_connected {
            self.disconnect(b"disconnect");
        }
    }
}

fn hexdump(level: Level, data: &[u8]) {
    if log_enabled!(level) {
        hexdump_iter(data).for_each(|s| log!(level, "{s}"));
    }
}

struct Warn<'a>(SocketAddr, &'a [u8]);

impl<W: std::fmt::Debug> warn::Warn<W> for Warn<'_> {
    fn warn(&mut self, w: W) {
        debug!("{}: {:?}", self.0, w);
        hexdump(Level::Debug, self.1);
    }
}

#[derive(Debug)]
pub enum ClientState {
    WaitingForMapInfo,
    /// Wait especially for map change packet
    /// (for map details package only)
    WaitingForMapChange {
        name: ReducedAsciiString,
        hash: Hash,
    },
    DownloadingMap {
        expected_size: Option<usize>,
        data: BTreeMap<usize, Vec<u8>>,
        name: ReducedAsciiString,
        sha256: Option<Hash>,
    },
    MapReady {
        name: ReducedAsciiString,
        hash: Hash,
    },
    RequestedLegacyServerInfo {
        name: ReducedAsciiString,
        hash: Hash,
        token: u8,
    },
    ReceivedLegacyServerInfo {
        name: ReducedAsciiString,
        hash: Hash,
    },
    SentServerInfo,
    StartInfoSent,
    Ingame,
}

#[derive(Debug, Default)]
pub struct ClientReady {
    pub con: bool,
    pub received_server_info: Option<ServerInfo>,
    pub client_con: bool,
}

pub struct ClientData {
    pub state: ClientState,
    pub ready: ClientReady,
    pub snap_manager: Manager,
    pub latest_input: snap_obj::PlayerInput,
    pub latest_char_input: CharacterInput,

    pub connect_time: Duration,

    pub player_info: NetworkCharacterInfo,

    pub latest_inputs: BTreeMap<i32, (CharacterInput, snap_obj::PlayerInput)>,
    pub server_client: ServerClientPlayer,
}

pub struct ProxyClient {
    pub socket: SocketClient,

    pub data: ClientData,
}

impl ProxyClient {
    pub fn new(
        player_info: NetworkCharacterInfo,
        socket: SocketClient,
        connect_time: Duration,
        id: u64,
        secondary_player: bool,
    ) -> Self {
        ProxyClient {
            socket,

            data: ClientData {
                state: ClientState::WaitingForMapInfo,
                ready: ClientReady {
                    client_con: true,
                    ..Default::default()
                },

                snap_manager: Default::default(),
                latest_input: Default::default(),
                latest_char_input: {
                    let mut inp = CharacterInput::default();

                    // assume dummy first, so snapshots are handled correctly.
                    if secondary_player {
                        inp.state
                            .input_method_flags
                            .set(CharacterInputMethodFlags::DUMMY);
                    }

                    inp
                },

                connect_time,

                player_info,

                latest_inputs: Default::default(),
                server_client: ServerClientPlayer {
                    id,
                    input_storage: Default::default(),
                },
            },
        }
    }
}

pub struct WarnPkt<'a, T: std::fmt::Debug>(pub T, pub &'a [u8]);

impl<T: std::fmt::Debug, W: std::fmt::Debug> warn::Warn<W> for WarnPkt<'_, T> {
    fn warn(&mut self, w: W) {
        debug!("{:?}: {:?}", self.0, w);
        hexdump(Level::Debug, self.1);
    }
}

impl ProxyClient {
    pub fn sends<'a, S: Into<System<'a>>>(&mut self, msg: S) {
        self.socket.sends(msg)
    }
    pub fn sendg<'a, G: Into<Game<'a>>>(&mut self, msg: G) {
        self.socket.sendg(msg)
    }
    pub fn flush(&mut self) {
        self.socket.flush();
    }
}
