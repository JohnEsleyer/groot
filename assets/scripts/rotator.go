type Rotator struct {
    YawSpeed float64
    Current  float64
}

var self = Rotator{
    YawSpeed: 1.5,
    Current:  0.0,
}

func OnUpdate(dt float64) {
    self.Current = self.Current + self.YawSpeed*dt
    groot.SetSelfRotation3D(self.Current*0.5, self.Current, 0.0)
}
