use std::{collections::BTreeMap, time::Duration};

use anyhow::anyhow;
use arrayvec::ArrayVec;
use base::{hash::Hash, reduced_ascii_str::ReducedAsciiString};
use game_interface::types::{character_info::NetworkCharacterInfo, input::CharacterInput};
use game_server::client::ServerClientPlayer;
use hexdump::hexdump_iter;
use libtw2_event_loop::{Chunk, PeerId};
use libtw2_gamenet_ddnet::{
    msg::{Connless, Game, System},
    snap_obj,
};
use libtw2_net::{net::ChunkOrEvent, Net};
use libtw2_packer::with_packer;
use libtw2_snapshot::Manager;
use libtw2_socket::{Addr, Socket};
use log::{debug, log, log_enabled, Level};

pub struct SocketClient {
    pub socket: Socket,
    pub net: Net<Addr>,
    pub server_pid: PeerId,
    pub skip_disconnect_on_drop: bool,
}

impl SocketClient {
    pub fn new(addr: Addr) -> anyhow::Result<SocketClient> {
        let mut socket = Socket::new().unwrap();
        let mut net = Net::client();
        let (server_pid, res) = net.connect(&mut socket, addr);

        res.map_err(|err| anyhow!(err)).map(|_| SocketClient {
            socket,
            net,
            server_pid,
            skip_disconnect_on_drop: false,
        })
    }
    pub fn run_once(&mut self, mut on_event: impl FnMut(&mut Self, ChunkOrEvent<'_, Addr>)) {
        let mut buf1: ArrayVec<[u8; 4096]> = ArrayVec::new();
        let mut buf2: ArrayVec<[u8; 4096]> = ArrayVec::new();

        self.net
            .tick(&mut self.socket)
            .for_each(|e| panic!("{:?}", e));

        self.socket.sleep(Some(Duration::from_micros(1))).unwrap();

        while let Some(res) = {
            buf1.clear();
            self.socket.receive(&mut buf1)
        } {
            let (addr, data) = res.unwrap();
            buf2.clear();
            let (iter, res) = self.net.feed(
                &mut self.socket,
                &mut Warn(addr, data),
                addr,
                data,
                &mut buf2,
            );
            res.unwrap();
            for mut chunk in iter {
                if !self.net.is_receive_chunk_still_valid(&mut chunk) {
                    continue;
                }
                if let ChunkOrEvent::Connect(pid) = chunk {
                    self.net.accept(&mut self.socket, pid).unwrap();
                } else {
                    on_event(self, chunk);
                }
            }
        }
    }
    pub fn disconnect(&mut self, reason: &[u8]) {
        self.net
            .disconnect(&mut self.socket, self.server_pid, reason)
            .unwrap();
    }
    fn send(&mut self, chunk: Chunk) {
        self.net.send(&mut self.socket, chunk).unwrap();
    }
    fn send_connless(&mut self, addr: Addr, data: &[u8]) {
        self.net
            .send_connless(&mut self.socket, addr, data)
            .unwrap();
    }
    pub fn flush(&mut self) {
        self.net.flush(&mut self.socket, self.server_pid).unwrap();
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
    pub fn sendc<'a, C: Into<Connless<'a>>>(&mut self, addr: Addr, msg: C) {
        fn inner(msg: Connless, addr: Addr, socket_client: &mut SocketClient) {
            let mut buf: ArrayVec<[u8; 2048]> = ArrayVec::new();
            with_packer(&mut buf, |p| msg.encode(p).unwrap());
            socket_client.send_connless(addr, &buf)
        }
        inner(msg.into(), addr, self)
    }
}

impl Drop for SocketClient {
    fn drop(&mut self) {
        if !self.skip_disconnect_on_drop {
            self.disconnect(b"disconnect");
        }
    }
}

fn hexdump(level: Level, data: &[u8]) {
    if log_enabled!(level) {
        hexdump_iter(data).for_each(|s| log!(level, "{}", s));
    }
}

struct Warn<'a>(Addr, &'a [u8]);

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
    SentServerInfo,
    StartInfoSent,
    Ingame,
}

#[derive(Debug, Default)]
pub struct ClientReady {
    pub con: bool,
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
                latest_char_input: Default::default(),

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
