type Vector2 struct {
    X float64
    Y float64
}

var pos = Vector2{X: 0, Y: 0}
var speed = 300.0

func OnUpdate(dt float64) {
    var inputX = groot.GetAxis("Horizontal")
    var inputY = groot.GetAxis("Vertical")

    pos.X = pos.X + inputX * speed * dt
    pos.Y = pos.Y + inputY * speed * dt

    groot.SetPosition(pos.X, pos.Y)

    if inputX != 0 || inputY != 0 {
        var msg = fmt.Sprintf("Groot Player at X: %f, Y: %f", pos.X, pos.Y)
        groot.Log(msg)
    }

    if groot.IsKeyDown("Space") {
        groot.Log("Spacebar pressed!")
        groot.SpawnEntity("JumpEffect")
    }
}