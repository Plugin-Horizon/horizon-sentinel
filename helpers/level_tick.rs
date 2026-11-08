// The level's tick (after its entities ticked): every Sentinel spawned in this level counts a
// tick, and those due (`update rate`) run their update, as Sentinel's trait `run()` does.
fn on_level_tick(game: &mut crate::game::Game<'_>, dimension: &str) {
    use crate::game::sentinel_host::Body;
    use horizon_client::sentinel as s;
    use horizon_client::sentinel::core::world::World;
    crate::game::sentinel_host::expire_arrows(game, dimension);
    if s::core::registry().sentinels.is_empty() {
        return;
    }
    let notices = std::mem::take(&mut s::core::registry().notices);
    for (player, line) in notices {
        game.native_tell(player, horizon_client::text::Text::literal(&line));
    }
    s::with_bridge(|bridge| {
        let views = game.sentinel_views(&*bridge);
        let due = s::core::registry().tick(bridge, Some(dimension));
        for npc in &due {
            let Some(eid) = bridge.entity_of(*npc) else { continue };
            let Some(me) = views.iter().find(|v| v.id == eid).cloned() else { continue };
            if let Some(m) = game.mobs.iter_mut().find(|m| m.id == eid as u32) {
                s::allow_ai(m);
            }
            let fresh = {
                let mut reg = s::core::registry();
                if reg.is_new_body(*npc, me.uuid) { reg.spawned(*npc, &me) } else { None }
            };
            if let Some((_, invincible)) = fresh {
                bridge.set_protected(*npc, invincible);
            }
            let guarded = {
                let reg = s::core::registry();
                let Some(sen) = reg.sentinels.get(npc) else { continue };
                let by_player = sen.data.guarding().and_then(|u| views.iter().find(|v| v.uuid == u).cloned());
                let by_npc = (sen.data.guarded_npc >= 0)
                    .then(|| bridge.entity_of(sen.data.guarded_npc))
                    .flatten()
                    .and_then(|e| views.iter().find(|v| v.id == e).cloned());
                by_player.or(by_npc)
            };
            let owner = bridge.owner(*npc);
            // `Level.getGameTime` of this level, through `native_level_game_time` (main has no `game_time`).
            let now = game.native_level(dimension).and_then(|l| game.native_level_game_time(l)).unwrap_or(0);
            let seed = (me.uuid as u64) ^ (now as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            // hooks-b-all `native_mob_navigating`, read once per tick (the brain's `World` asks
            // through `&self`).
            let navigating = game.native_mob_navigating(dimension, eid as u32);
            let mut body = Body {
                navigating,
                game: &mut *game,
                npc: *npc,
                mob: eid as u32,
                dim: dimension.to_owned(),
                views: &views,
                bridge: &mut *bridge,
                random: seed,
            };
            if let Some((health, _)) = fresh {
                body.set_max_health(health);
                body.set_health(health);
            }
            s::core::registry().update(*npc, &me, guarded, owner, &mut body);
        }
        s::core::registry().end_tick(dimension, !due.is_empty());
    });
}
