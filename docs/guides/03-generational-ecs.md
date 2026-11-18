# Guide 03 — Generational ECS & Entity Management

The `pkgs/groot/ecs` package provides a cache-coherent, generational entity component system built on top of Ube's native `Pool[T]`.

## Spawning Entities

```ube
import "../../pkgs/groot/ecs" as ecs
import "../../pkgs/groot/color" as color

let mut world = ecs::world_new(1024)

// Spawns entity and returns a generational handle
let player_handle = world.create_sprite_rgba(
    100.0, 200.0,  // position x, y
    32.0, 32.0,    // size w, h
    0.2, 0.8, 0.3, 1.0 // color rgba
)
```

## Safe Entity Queries

Generational indices guarantee that querying an entity via a handle that has been despawned returns `none` instead of reading recycled memory:

```ube
if let ent = world.get(player_handle) {
    if let spr = ent.sprite {
        // Safe to read transform and sprite properties
        println(ent.transform.position.x)
    }
}
```

## Despawning

```ube
world.despawn(player_handle)
// Future world.get(player_handle) calls will safely return none!
```
