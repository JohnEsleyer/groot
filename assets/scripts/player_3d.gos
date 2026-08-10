type Player struct {
    Speed float64
    Hue   float64
}

var self = Player{
    Speed: 8.0,
    Hue:   0.0,
}

func OnUpdate(dt float64) {
    var pos = groot.GetSelfPosition()
    var moveX = groot.GetAxis("Horizontal")
    var moveZ = -groot.GetAxis("Vertical")

    var nx = pos[0] + moveX*self.Speed*dt
    var ny = pos[1]
    var nz = pos[2] + moveZ*self.Speed*dt

    groot.SetSelfPosition(nx, ny, nz)

    if groot.IsKeyPressed("Space") {
        self.Hue = self.Hue + 0.25
        if self.Hue > 1.0 {
            self.Hue = 0.0
        }
        groot.SetSelfMaterialColor(self.Hue, 0.8, 1.0-self.Hue, 1.0)
        groot.Log("Player material color changed via GoScript!")
    }

    groot.SetSelfCollider(1.0, 1.5, 1.0)
}
