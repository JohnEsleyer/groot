// player.gs — Hybrid Component-Behavior Player Script
// Demonstrates: struct state, receiver methods, self-context, entity queries,
// event emitting, collision detection, and debug gizmo drawing.

type Player struct {
    Speed float64
    Hp    int
}

var player = Player{
    Speed: 350.0,
    Hp:    100,
}

func OnUpdate(dt float64) {
    var selfId = groot.GetSelfEntity()
    var pos = groot.GetSelfPosition()
    var px = pos[0]
    var py = pos[1]

    // 1. Movement Input
    var inputX = groot.GetAxis("Horizontal")
    var inputY = groot.GetAxis("Vertical")
    var newX = groot.Clamp(px + inputX * player.Speed * dt, -580.0, 580.0)
    var newY = groot.Clamp(py + inputY * player.Speed * dt, -320.0, 320.0)
    groot.SetSelfPosition(newX, newY)

    // 2. Mouse Ray Visualization
    var mousePos = groot.GetMousePosition()
    var mx = mousePos[0]
    var my = mousePos[1]
    if groot.IsMouseButtonDown(0) {
        groot.DrawDebugLine(newX, newY, mx, my, 0.0, 0.9, 1.0)
    }

    // 3. Collision Check against Enemy (ID #2)
    var enemyPos = groot.GetEntityPosition(2)
    var epx = enemyPos[0]
    var epy = enemyPos[1]
    var isHit = groot.CheckCollisionRecs(
        newX - 30.0, newY - 30.0, 60.0, 60.0,
        epx - 30.0, epy - 30.0, 60.0, 60.0,
    )
    if isHit {
        groot.SetSelfColor(1.0, 0.2, 0.2, 1.0)
        player.TakeDamage(1)
    } else {
        groot.SetSelfColor(0.1, 0.8, 0.3, 1.0)
    }

    // 4. Debug Circle
    groot.DrawDebugCircle(newX, newY, 35.0, 0.2, 1.0, 0.5)

    // 5. Distance Query
    var dist = groot.GetDistance(selfId, 2)
    if dist < 120.0 {
        groot.Warn("Enemy nearby! Distance: " + fmt.Sprintf("%.1f", dist))
    }

    // 6. Actions
    if groot.IsKeyPressed("Space") {
        groot.Log("Spacebar pressed!")
        groot.PlaySound("assets/sounds/jump.wav")
        groot.SpawnEntity("effects/jump.gs", newX, newY - 30.0)
    }

    groot.Log(fmt.Sprintf("pos=(%.1f,%.1f) hp=%d dist=%.1f",
        newX, newY, player.Hp, dist))
}

func (p *Player) TakeDamage(amount int) {
    p.Hp -= amount
    if p.Hp <= 0 {
        groot.EmitEvent("PlayerDied", groot.GetSelfEntity())
        groot.DestroySelf()
    }
}
