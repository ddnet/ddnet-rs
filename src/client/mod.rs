pub mod client;
pub mod game;
mod game_events;
mod input;
pub mod localplayer;
mod overlays;
pub mod spatial_chat;
pub mod ui;

#[cfg(test)]
mod test {
    use game_base::network::messages::{MsgClInputPlayerChain, PlayerInputChainable};
    use network::network::{
        connections::NetworkConnectionIdCounter, packet_compressor::ZstdNetworkPacketCompressor,
        plugins::NetworkPluginPacket,
    };
    use pool::mt_datatypes::PoolVec;

    #[tokio::test]
    async fn input() {
        let mut cur_inp = PlayerInputChainable {
            inp: Default::default(),
            for_monotonic_tick: 0,
        };

        let mut data = Vec::new();
        let mut def = bincode::serde::encode_to_vec(
            PlayerInputChainable::default(),
            bincode::config::standard().with_fixed_int_encoding(),
        )
        .unwrap();
        for _ in 0..5 {
            let inp = bincode::serde::encode_to_vec(
                cur_inp,
                bincode::config::standard().with_fixed_int_encoding(),
            )
            .unwrap();

            bin_patch::diff_exact_size(&def, &inp, &mut data).unwrap();

            cur_inp.inp.inc_version();
            cur_inp.for_monotonic_tick += 1;
            def = inp;
        }

        let comp = ZstdNetworkPacketCompressor::new();
        let gen = NetworkConnectionIdCounter::default();

        // this should be smaller than the number of inputs saved on the server
        let as_diff = true;

        comp.prepare_write(&gen.get_next(0), &mut data)
            .await
            .unwrap();
        dbg!(data.len());

        let mut msg = bincode::serde::encode_to_vec(
            MsgClInputPlayerChain {
                data: PoolVec::from_without_pool(data),
                diff_id: Some(0),
                as_diff,
            },
            bincode::config::standard(),
        )
        .unwrap();

        comp.prepare_write(&gen.get_next(0), &mut msg)
            .await
            .unwrap();

        dbg!(msg.len());
    }
}
