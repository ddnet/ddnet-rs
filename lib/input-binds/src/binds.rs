use std::{collections::BTreeSet, fmt::Debug, hash::Hash, iter::Peekable};

use base::linked_hash_map_view::{FxLinkedHashMap, FxLinkedHashSet};
use hiarc::Hiarc;
use pool::{datatypes::PoolFxLinkedHashSet, pool::Pool};
use serde::{Deserialize, Serialize};
pub use winit::{event::MouseButton, keyboard::KeyCode, keyboard::PhysicalKey};

#[derive(
    Debug, Hiarc, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum MouseExtra {
    WheelDown,
    WheelUp,
}

#[derive(
    Debug, Hiarc, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum BindKey {
    Key(PhysicalKey),
    Mouse(MouseButton),
    Extra(MouseExtra),
}

#[derive(Debug, Clone)]
pub enum BindTarget<T> {
    Scancode(KeyTarget<T>),
    Actions(Vec<T>),
    ScancodeAndActions((KeyTarget<T>, Vec<T>)),
}

pub type KeyTarget<F> = FxLinkedHashMap<BindKey, BindTarget<F>>;

pub struct BindsProcessResult<F> {
    pub click_actions: PoolFxLinkedHashSet<F>,
    pub press_actions: PoolFxLinkedHashSet<F>,
    pub unpress_actions: PoolFxLinkedHashSet<F>,
    pub cur_actions: PoolFxLinkedHashSet<F>,
}

#[derive(Debug)]
pub struct Binds<T> {
    keys: KeyTarget<T>,
    cur_keys_pressed_is_order: BTreeSet<BindKey>,

    /// actions caused by a press + release of a key
    click_actions: PoolFxLinkedHashSet<T>,
    press_actions: PoolFxLinkedHashSet<T>,
    unpress_actions: PoolFxLinkedHashSet<T>,
    helper_process_pool: Pool<FxLinkedHashSet<T>>,
}

impl<T> Default for Binds<T> {
    fn default() -> Self {
        let helper_process_pool = Pool::with_capacity(3);
        Self {
            keys: Default::default(),
            cur_keys_pressed_is_order: Default::default(),
            click_actions: helper_process_pool.new(),
            press_actions: helper_process_pool.new(),
            unpress_actions: helper_process_pool.new(),
            helper_process_pool,
        }
    }
}

impl<T: Debug + Clone + Hash + PartialEq + Eq> Binds<T> {
    pub fn handle_key_down(&mut self, code: &BindKey) {
        let BindsProcessResult { cur_actions, .. } = self.process_impl(false);
        self.cur_keys_pressed_is_order.insert(*code);
        let BindsProcessResult {
            cur_actions: new_actions,
            ..
        } = self.process_impl(false);
        // create diff between both
        new_actions.difference(&cur_actions).for_each(|action| {
            self.press_actions.insert(action.clone());
        });
    }

    pub fn handle_key_up(&mut self, code: &BindKey) {
        let BindsProcessResult { cur_actions, .. } = self.process_impl(false);
        self.cur_keys_pressed_is_order.remove(code);
        let BindsProcessResult {
            cur_actions: new_actions,
            ..
        } = self.process_impl(false);
        // create diff between both
        cur_actions.difference(&new_actions).for_each(|action| {
            self.click_actions.insert(action.clone());
        });
        // and same for unpress actions
        cur_actions.difference(&new_actions).for_each(|action| {
            self.unpress_actions.insert(action.clone());
        });
    }

    fn process_impl(&mut self, consume_events: bool) -> BindsProcessResult<T> {
        // tries to find the bind with the longest chain possible
        // the first key(s) can be ignored (`can_ignore_keys`), because it might not have any bind at all
        fn find_longest_chain_action<'a, F: Debug>(
            mut key_iter: std::collections::btree_set::Iter<'a, BindKey>,
            keys: &'a KeyTarget<F>,
            can_ignore_keys: bool,
        ) -> Option<(&'a Vec<F>, std::collections::btree_set::Iter<'a, BindKey>)> {
            match key_iter.next() {
                Some(next_key) => {
                    match keys.get(next_key) {
                        Some(key_binds) => match key_binds {
                            BindTarget::Scancode(cur_scan) => {
                                find_longest_chain_action(key_iter, cur_scan, false)
                            }
                            BindTarget::Actions(actions) => Some((actions, key_iter)),
                            BindTarget::ScancodeAndActions((cur_scan, actions)) => {
                                let res =
                                    find_longest_chain_action(key_iter.clone(), cur_scan, false);
                                // prefer longest chain if available
                                if res.is_some() {
                                    res
                                } else {
                                    Some((actions, key_iter))
                                }
                            }
                        },
                        // if nothing was found at this key, try the
                        None => {
                            if can_ignore_keys {
                                find_longest_chain_action(key_iter, keys, true)
                            } else {
                                None
                            }
                        }
                    }
                }
                None => None,
            }
        }

        let mut cur_actions = self.helper_process_pool.new();
        let mut key_iter = self.cur_keys_pressed_is_order.iter();
        while let Some((actions, key_iter_next)) =
            find_longest_chain_action(key_iter, &self.keys, true)
        {
            key_iter = key_iter_next;
            actions.iter().for_each(|f| {
                cur_actions.insert(f.clone());
            });
        }

        BindsProcessResult {
            click_actions: if consume_events {
                std::mem::replace(&mut self.click_actions, self.helper_process_pool.new())
            } else {
                self.helper_process_pool.new()
            },
            press_actions: if consume_events {
                std::mem::replace(&mut self.press_actions, self.helper_process_pool.new())
            } else {
                self.helper_process_pool.new()
            },
            unpress_actions: if consume_events {
                std::mem::replace(&mut self.unpress_actions, self.helper_process_pool.new())
            } else {
                self.helper_process_pool.new()
            },
            cur_actions,
        }
    }

    pub fn process(&mut self) -> BindsProcessResult<T> {
        self.process_impl(true)
    }

    pub fn register_bind(&mut self, bind_keys: &[BindKey], actions: T) {
        let keys = &mut self.keys;

        fn insert_into_keys<F: Clone>(
            mut key_iter: Peekable<std::collections::btree_set::Iter<'_, BindKey>>,
            keys: &mut KeyTarget<F>,
            action: F,
        ) {
            if let Some(scancode) = key_iter.next() {
                if key_iter.peek().is_some() {
                    if let Some(cur) = keys.get_mut(scancode) {
                        match cur {
                            BindTarget::Scancode(cur_scan) => {
                                insert_into_keys(key_iter, cur_scan, action)
                            }
                            BindTarget::Actions(cur_action) => {
                                let repl_action = cur_action.clone();
                                *cur = BindTarget::ScancodeAndActions((
                                    Default::default(),
                                    repl_action,
                                ));
                                if let BindTarget::ScancodeAndActions((cur_scan, _)) = cur {
                                    insert_into_keys(key_iter, cur_scan, action)
                                }
                            }
                            BindTarget::ScancodeAndActions((cur_scan, _)) => {
                                insert_into_keys(key_iter, cur_scan, action)
                            }
                        }
                    } else {
                        let mut inner_keys = Default::default();
                        insert_into_keys(key_iter, &mut inner_keys, action);
                        keys.insert(*scancode, BindTarget::Scancode(inner_keys));
                    }
                } else if let Some(cur) = keys.get_mut(scancode) {
                    match cur {
                        BindTarget::Scancode(cur_scan) => {
                            let repl_scan = cur_scan.clone();
                            *cur = BindTarget::ScancodeAndActions((repl_scan, vec![action]))
                        }
                        BindTarget::Actions(actions) => actions.push(action),
                        BindTarget::ScancodeAndActions((_, actions)) => actions.push(action),
                    }
                } else {
                    keys.insert(*scancode, BindTarget::Actions(vec![action]));
                }
            }
        }
        let keys_in_order: BTreeSet<BindKey> =
            bind_keys.iter().copied().collect::<BTreeSet<BindKey>>();
        insert_into_keys(keys_in_order.iter().peekable(), keys, actions);
    }
}
