pub mod simulation_pipe {
    use std::marker::PhantomData;
    use std::ops::{ControlFlow, Deref};

    use base::linked_hash_map_view::FxLinkedHashMap;
    use game_interface::events::{
        EventIdGenerator, GameWorldEffectEvent, GameWorldEntityEffectEvent,
        GameWorldEntitySoundEvent, GameWorldEvent, GameWorldEvents, GameWorldSoundEvent,
    };
    use game_interface::pooling::GamePooling;
    use game_interface::types::id_types::{
        CharacterId, CtfFlagId, LaserId, PickupId, ProjectileId, StageId,
    };
    use hiarc::{hi_closure, HiFnMut};
    use hiarc::{hiarc_safer_rc_refcell, Hiarc};
    use math::math::vector::vec2;
    use serde::{Deserialize, Serialize};

    use crate::entities::character::character::{CharactersView, CharactersViewMut};
    use crate::entities::character::core::character_core::{Core, CoreReusable};
    use crate::entities::character::pos::character_pos::{
        CharacterPos, CharacterPositionPlayfield,
    };
    use crate::entities::flag::flag::Flags;
    use crate::events::events::{
        CharacterTickEvent, FlagEvent, LaserEvent, PickupEvent, ProjectileEvent,
    };
    use crate::world::world::GameObjectsWorld;
    use crate::{
        entities::character::character::Characters,
        events::events::CharacterEvent,
        world::world::{GameWorld, WorldPool},
    };

    use super::super::{
        collision::collision::Collision, entities::character::character::Character,
    };

    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Default, Hiarc)]
    pub struct GameWorldPendingEvents {
        evs: Vec<GameWorldEvent>,

        _p: PhantomData<GameObjectsWorld>,
        _g: PhantomData<GamePooling>,
        _e: PhantomData<EventIdGenerator>,
        _s: PhantomData<pool::mt_datatypes::PoolFxLinkedHashMap<StageId, GameWorldEvents>>,
    }

    #[hiarc_safer_rc_refcell]
    impl GameWorldPendingEvents {
        pub fn push(&mut self, ev: GameWorldEvent) {
            self.evs.push(ev);
        }

        pub fn push_sound(
            &mut self,
            owner_id: Option<CharacterId>,
            pos: Option<vec2>,
            ev: GameWorldEntitySoundEvent,
        ) {
            self.evs.push(GameWorldEvent::Sound(GameWorldSoundEvent {
                ev,
                owner_id,
                pos: pos.map(|pos| pos / 32.0),
            }));
        }

        pub fn push_effect(
            &mut self,
            owner_id: Option<CharacterId>,
            pos: vec2,
            ev: GameWorldEntityEffectEvent,
        ) {
            self.evs.push(GameWorldEvent::Effect(GameWorldEffectEvent {
                ev,
                owner_id,
                pos: pos / 32.0,
            }));
        }

        pub fn take(&mut self) -> Vec<GameWorldEvent> {
            std::mem::take(&mut self.evs)
        }

        pub fn clear(&mut self) {
            self.evs.clear();
        }

        pub fn set(&mut self, evs: Vec<GameWorldEvent>) {
            self.evs = evs;
        }

        pub fn for_each<F>(&self, mut f: F)
        where
            for<'a> F: HiFnMut<&'a GameWorldEvent, ()>,
        {
            self.evs.iter().for_each(move |ev| f.call_mut(ev))
        }

        pub fn for_each_evs<F>(&self, mut f: F)
        where
            for<'a> F: HiFnMut<&'a Vec<GameWorldEvent>, ()>,
        {
            f.call_mut(&self.evs);
        }
    }

    /// Simulation events in a single stage
    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc)]
    pub struct GameStagePendingEvents {
        events: GameWorldPendingEvents,

        // match manager should have higher hierarchy than world
        _py: PhantomData<GameWorld>,
    }

    #[hiarc_safer_rc_refcell]
    impl Default for GameStagePendingEvents {
        fn default() -> Self {
            Self::new()
        }
    }

    #[hiarc_safer_rc_refcell]
    impl GameStagePendingEvents {
        pub fn new() -> Self {
            Self {
                events: Default::default(),

                _py: Default::default(),
            }
        }

        pub fn push(&mut self, ev: GameWorldEvent) {
            self.events.push(ev);
        }

        pub fn take(&self) -> Vec<GameWorldEvent> {
            self.events.take()
        }

        pub fn clear(&self) {
            self.events.clear()
        }

        pub fn clone_evs(&self) -> GameWorldPendingEvents {
            self.events.clone()
        }

        pub fn for_each_evs<F>(&self, f: F)
        where
            for<'a> F: HiFnMut<&'a Vec<GameWorldEvent>, ()>,
        {
            self.events.for_each_evs(f)
        }
    }

    /// The game events shared by stage and state
    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Default, Hiarc)]
    pub struct GameWorldPendingEventsInner(FxLinkedHashMap<StageId, GameStagePendingEvents>);

    #[hiarc_safer_rc_refcell]
    impl GameWorldPendingEventsInner {
        pub fn remove(&mut self, id: &StageId) {
            self.0.remove(id);
        }
        pub fn insert(&mut self, id: StageId, evs: GameStagePendingEvents) {
            self.0.insert(id, evs);
        }

        pub fn clear_events(&mut self) {
            for stage_evs in self.0.values_mut() {
                stage_evs.clear();
            }
        }

        #[inline]
        pub fn for_each<F>(&self, mut f: F)
        where
            for<'a> F: HiFnMut<(&'a StageId, &'a Vec<GameWorldEvent>), ()>,
        {
            self.0.iter().for_each(move |(id, ev)| {
                let f = &mut f;
                ev.for_each_evs(hi_closure!(
                    <F: for<'b> HiFnMut<(&'b StageId, &'b Vec<GameWorldEvent>), ()>>,
                    [f: &mut F, id: &StageId],
                    |evs: &Vec<GameWorldEvent>| -> () {
                        f.call_mut((id, evs));
                    }
                ));
            })
        }
    }

    /// This is a game event wrapper specifically for stages.
    /// When dropped it "unregisteres" from the global game events
    #[derive(Debug, Hiarc)]
    pub struct GameStagePendingEventsRaii {
        stage_id: StageId,
        events: GameStagePendingEvents,

        events_container: GameWorldPendingEventsInner,
    }

    impl GameStagePendingEventsRaii {
        pub fn new(events_container: GameWorldPendingEventsInner, stage_id: StageId) -> Self {
            let events = GameStagePendingEvents::default();
            events_container.insert(stage_id, events.clone());
            Self {
                stage_id,
                events,
                events_container,
            }
        }
    }

    impl Deref for GameStagePendingEventsRaii {
        type Target = GameStagePendingEvents;

        fn deref(&self) -> &Self::Target {
            &self.events
        }
    }

    impl Drop for GameStagePendingEventsRaii {
        fn drop(&mut self) {
            self.events_container.remove(&self.stage_id);
        }
    }

    /// Game events that are pending to be send to a client
    #[derive(Debug, Default, Hiarc)]
    pub struct GamePendingEvents {
        events: GameWorldPendingEventsInner,
    }

    impl GamePendingEvents {
        pub fn init_stage(&mut self, stage_id: StageId) -> GameStagePendingEventsRaii {
            let events_container = self.events.clone();
            GameStagePendingEventsRaii::new(events_container, stage_id)
        }
    }

    impl Deref for GamePendingEvents {
        type Target = GameWorldPendingEventsInner;

        fn deref(&self) -> &Self::Target {
            &self.events
        }
    }

    /// Game internal simulation events
    #[derive(Debug, Hiarc, Serialize, Deserialize)]
    pub enum SimulationEventWorldEntityType {
        Character {
            ev: CharacterEvent,
        },
        Projectile {
            id: ProjectileId,
            ev: ProjectileEvent,
        },
        Pickup {
            id: PickupId,
            ev: PickupEvent,
        },
        Flag {
            id: CtfFlagId,
            ev: FlagEvent,
        },
        Laser {
            id: LaserId,
            ev: LaserEvent,
        },
    }

    #[derive(Debug, Hiarc, Serialize, Deserialize)]
    pub struct SimulationEventWorldEntity {
        pub ev: SimulationEventWorldEntityType,
    }

    /// Game internal simulation events
    #[derive(Debug, Hiarc, Serialize, Deserialize)]
    pub enum SimulationWorldEvent {
        Entity(SimulationEventWorldEntity),
    }

    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Default, Hiarc)]
    pub struct SimulationWorldEvents {
        evs: Vec<SimulationWorldEvent>,

        _p: PhantomData<GameObjectsWorld>,
        _g: PhantomData<GamePooling>,
        _e: PhantomData<EventIdGenerator>,
        _s: PhantomData<pool::mt_datatypes::PoolFxLinkedHashMap<StageId, GameWorldEvents>>,
    }

    #[hiarc_safer_rc_refcell]
    impl SimulationWorldEvents {
        pub fn push(&mut self, ev: SimulationWorldEvent) {
            self.evs.push(ev);
        }

        pub fn push_world(&mut self, ev: SimulationEventWorldEntityType) {
            self.evs
                .push(SimulationWorldEvent::Entity(SimulationEventWorldEntity {
                    ev,
                }));
        }

        pub fn take(&mut self) -> Vec<SimulationWorldEvent> {
            std::mem::take(&mut self.evs)
        }

        pub fn clear(&mut self) {
            self.evs.clear();
        }

        pub fn set(&mut self, evs: Vec<SimulationWorldEvent>) {
            self.evs = evs;
        }

        pub fn for_each<F>(&self, mut f: F)
        where
            for<'a> F: HiFnMut<&'a SimulationWorldEvent, ()>,
        {
            self.evs.iter().for_each(move |ev| f.call_mut(ev))
        }

        pub fn for_each_evs<F>(&self, mut f: F)
        where
            for<'a> F: HiFnMut<&'a Vec<SimulationWorldEvent>, ()>,
        {
            f.call_mut(&self.evs);
        }
    }

    /// Simulation events in a single stage
    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc)]
    pub struct SimulationStageEvents {
        events: SimulationWorldEvents,

        // match manager should have higher hierarchy than world
        _py: PhantomData<GameWorld>,
    }

    #[hiarc_safer_rc_refcell]
    impl Default for SimulationStageEvents {
        fn default() -> Self {
            Self::new()
        }
    }

    #[hiarc_safer_rc_refcell]
    impl SimulationStageEvents {
        pub fn new() -> Self {
            Self {
                events: Default::default(),

                _py: Default::default(),
            }
        }

        pub fn push(&mut self, ev: SimulationWorldEvent) {
            self.events.push(ev);
        }

        pub fn take(&self) -> Vec<SimulationWorldEvent> {
            self.events.take()
        }

        pub fn clear(&self) {
            self.events.clear()
        }

        pub fn clone_evs(&self) -> SimulationWorldEvents {
            self.events.clone()
        }

        pub fn for_each<F>(&self, mut f: F)
        where
            for<'a> F: HiFnMut<&'a SimulationWorldEvent, ()>,
        {
            let evs = self.events.take();
            evs.iter().for_each(|ev| f.call_mut(ev));
            self.events.set(evs);
        }

        pub fn for_each_evs<F>(&self, f: F)
        where
            for<'a> F: HiFnMut<&'a Vec<SimulationWorldEvent>, ()>,
        {
            self.events.for_each_evs(f)
        }
    }

    pub struct SimulationPipe<'a> {
        pub collision: &'a Collision,
    }

    impl<'a> SimulationPipe<'a> {
        pub fn new(collision: &'a Collision) -> Self {
            Self { collision }
        }
    }

    pub struct SimulationPipeStage<'a> {
        // should only be true inside a client's simulation pipe
        pub is_prediction: bool,

        pub collision: &'a Collision,

        pub stage_id: &'a StageId,

        pub world_pool: &'a WorldPool,
    }

    impl<'a> SimulationPipeStage<'a> {
        pub fn new(
            is_prediction: bool,
            collision: &'a Collision,
            stage_id: &'a StageId,
            world_pool: &'a WorldPool,
        ) -> Self {
            Self {
                is_prediction,
                collision,
                stage_id,
                world_pool,
            }
        }
    }

    pub trait SimulationPipeCharactersGetter {
        fn for_other_characters_in_range_mut(
            &mut self,
            char_pos: &vec2,
            radius: f32,
            for_each_func: &mut dyn FnMut(&mut Character),
        );
        fn get_other_character_id_and_cores_iter_by_ids_mut(
            &mut self,
            ids: &[CharacterId],
            for_each_func: &mut dyn FnMut(
                &CharacterId,
                &mut Core,
                &mut CoreReusable,
                &mut CharacterPos,
            ) -> ControlFlow<()>,
        ) -> ControlFlow<()>;
        fn get_other_character_pos_by_id(&self, other_char_id: &CharacterId) -> &vec2;
        fn get_other_character_by_id_mut(&mut self, other_char_id: &CharacterId) -> &mut Character;
    }

    pub struct SimulationPipeCharacter<'a> {
        pub characters: &'a mut dyn SimulationPipeCharactersGetter,
        pub entity_events: &'a mut Vec<CharacterTickEvent>,

        pub collision: &'a Collision,
    }

    impl<'a> SimulationPipeCharacter<'a> {
        pub fn new(
            characters: &'a mut dyn SimulationPipeCharactersGetter,
            entity_events: &'a mut Vec<CharacterTickEvent>,
            collision: &'a Collision,
        ) -> Self {
            Self {
                characters,
                entity_events,
                collision,
            }
        }
    }

    pub struct SimulationPipeCharacters<'a> {
        pub characters: &'a mut Characters,
        pub owner_character: CharacterId,
    }

    impl SimulationPipeCharacters<'_> {
        pub fn get_characters_except_owner(
            &mut self,
        ) -> CharactersViewMut<impl Fn(&CharacterId) -> bool + '_, impl Fn(&Character) -> bool + '_>
        {
            CharactersViewMut::new(
                self.characters,
                |id| *id != self.owner_character,
                |c| !c.phased.is_phased(),
            )
        }
        pub fn get_characters(
            &mut self,
        ) -> CharactersViewMut<impl Fn(&CharacterId) -> bool, impl Fn(&Character) -> bool> {
            CharactersViewMut::new(self.characters, |_| true, |c| !c.phased.is_phased())
        }
        pub fn get_owner_character_view(
            &mut self,
        ) -> CharactersViewMut<impl Fn(&CharacterId) -> bool + '_, impl Fn(&Character) -> bool + '_>
        {
            CharactersViewMut::new(
                self.characters,
                |id| *id == self.owner_character,
                |c| !c.phased.is_phased(),
            )
        }
    }

    pub struct SimulationPipeProjectile<'a> {
        pub collision: &'a Collision,

        pub characters_helper: SimulationPipeCharacters<'a>,
        pub field: &'a CharacterPositionPlayfield,
    }

    impl<'a> SimulationPipeProjectile<'a> {
        pub fn new(
            collision: &'a Collision,
            characters: &'a mut Characters,
            owner_character: CharacterId,
            field: &'a CharacterPositionPlayfield,
        ) -> Self {
            Self {
                collision,
                characters_helper: SimulationPipeCharacters {
                    characters,
                    owner_character,
                },
                field,
            }
        }
    }

    pub struct SimulationPipeOwnerlessCharacters<'a> {
        characters: &'a mut Characters,
    }

    impl SimulationPipeOwnerlessCharacters<'_> {
        pub fn characters(
            &self,
        ) -> CharactersView<impl Fn(&CharacterId) -> bool + '_, impl Fn(&Character) -> bool + '_>
        {
            CharactersView::new(self.characters, |_| true, |v| !v.phased.is_phased())
        }

        pub fn characters_mut(
            &mut self,
        ) -> CharactersViewMut<impl Fn(&CharacterId) -> bool + '_, impl Fn(&Character) -> bool + '_>
        {
            CharactersViewMut::new(self.characters, |_| true, |v| !v.phased.is_phased())
        }
    }

    pub struct SimulationPipePickup<'a> {
        pub characters: SimulationPipeOwnerlessCharacters<'a>,
        pub field: &'a CharacterPositionPlayfield,
    }

    impl<'a> SimulationPipePickup<'a> {
        pub fn new(characters: &'a mut Characters, field: &'a CharacterPositionPlayfield) -> Self {
            Self {
                characters: SimulationPipeOwnerlessCharacters { characters },
                field,
            }
        }
    }

    pub struct SimulationPipeFlag<'a> {
        pub collision: &'a Collision,

        pub characters: SimulationPipeOwnerlessCharacters<'a>,
        pub field: &'a CharacterPositionPlayfield,

        pub other_team_flags: &'a Flags,

        pub is_prediction: bool,
    }

    impl<'a> SimulationPipeFlag<'a> {
        pub fn new(
            collision: &'a Collision,
            characters: &'a mut Characters,
            field: &'a CharacterPositionPlayfield,
            other_team_flags: &'a Flags,
            is_prediction: bool,
        ) -> Self {
            Self {
                collision,
                characters: SimulationPipeOwnerlessCharacters { characters },
                field,
                is_prediction,
                other_team_flags,
            }
        }
    }

    pub struct SimulationPipeLaser<'a> {
        pub collision: &'a Collision,

        pub characters_helper: SimulationPipeCharacters<'a>,
        pub field: &'a CharacterPositionPlayfield,
    }

    impl<'a> SimulationPipeLaser<'a> {
        pub fn new(
            collision: &'a Collision,
            characters: &'a mut Characters,
            owner_character: CharacterId,
            field: &'a CharacterPositionPlayfield,
        ) -> Self {
            Self {
                collision,
                characters_helper: SimulationPipeCharacters {
                    characters,
                    owner_character,
                },
                field,
            }
        }
    }
}
