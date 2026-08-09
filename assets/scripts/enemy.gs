// enemy.gs — Hybrid Component-Behavior Enemy Script
// Demonstrates: struct state, receiver methods, self-context, distance queries,
// collision rectangles, event emitting, and debug visualization.

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
        groot.DrawDebugRect(newX, py, 70.0, 70.0, 1.0, 0.3, 0.0)
        groot.EmitEvent("EnemyAlert", dist)
    } else {
        groot.DrawDebugRect(newX, py, 60.0, 60.0, 0.8, 0.2, 0.8)
    }

    // 3. Debug circle
    groot.DrawDebugCircle(newX, py, 25.0, 0.5, 0.5, 0.0)

    groot.Log(fmt.Sprintf("enemy pos=(%.1f,%.1f) timer=%.1f dist=%.1f",
        newX, py, enemy.Timer, dist))
}
