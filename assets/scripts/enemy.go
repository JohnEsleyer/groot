// enemy.go — Hybrid Component-Behavior Enemy Script
// Demonstrates: struct state, receiver methods, self-context, distance queries,
// collider data, event emitting. All rendering is handled by the host engine.

type Enemy struct {
    PatrolSpeed float64
    Timer       float64
    PatrolRange float64
}

var enemy = Enemy{
    PatrolSpeed: 2.0,
    Timer:       0.0,
    PatrolRange: 220.0,
}

func OnUpdate(dt float64) {
    var selfId = groot.GetSelfEntity()
    var pos = groot.GetSelfPosition()
    var px = pos[0]
    var py = pos[1]

    // 1. Sine-wave Patrol
    enemy.Timer = enemy.Timer + dt
    var newX = math.Sin(enemy.Timer * enemy.PatrolSpeed) * enemy.PatrolRange
    groot.SetSelfPosition(newX, py)

    // 2. Distance Query to Player (ID #1)
    var dist = groot.GetDistance(selfId, 1)
    if dist < 120.0 {
        groot.Warn(fmt.Sprintf("Player detected! Distance: %.1f", dist))
        // Wider detection hitbox as data — the engine renders the overlay.
        groot.SetSelfCollider(70.0, 70.0)
        groot.EmitEvent("EnemyAlert", dist)
    } else {
        groot.SetSelfCollider(60.0, 60.0)
    }

    groot.Log(fmt.Sprintf("enemy pos=(%.1f,%.1f) timer=%.1f dist=%.1f",
        newX, py, enemy.Timer, dist))
}
