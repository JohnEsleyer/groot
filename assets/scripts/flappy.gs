type Pipe struct {
    X float64
    GapY float64
    GapSize float64
    Passed int
}

var gravity = -980.0
var jumpImpulse = 320.0
var pipeSpeed = 160.0
var birdX = -50.0
var birdRadius = 16.0
var pipeWidth = 52.0
var groundY = -210.0
var ceilingY = 280.0

var birdY = 0.0
var velocity = 0.0
var score = 0
var highScore = 0
var gameState = 0
var spaceWasDown = false

var pipes = []Pipe{}

func SpawnPipe(xPos float64) {
    var randomOffset = (rand.Float() - 0.5) * 200.0
    var newPipe = Pipe{
        X: xPos,
        GapY: randomOffset,
        GapSize: 130.0,
        Passed: 0,
    }
    pipes = append(pipes, newPipe)
}

func ResetGame() {
    birdY = 0.0
    velocity = 0.0
    score = 0
    gameState = 1
    pipes = []Pipe{}

    SpawnPipe(300.0)
    SpawnPipe(500.0)
    SpawnPipe(700.0)

    groot.SetScoreDisplay(score, highScore)
    groot.Log("FLAPPY BIRD STARTED")
}

func OnUpdate(dt float64) {
    var spaceDown = groot.IsKeyDown("Space")
    var spacePressed = spaceDown && !spaceWasDown
    spaceWasDown = spaceDown

    if gameState == 0 {
        birdY = math.Sin(time.Now() * 5.0) * 12.0
        groot.SetPosition(birdX, birdY)
        groot.SetScoreDisplay(score, highScore)

        if spacePressed {
            ResetGame()
            velocity = jumpImpulse
        }
        return
    }

    if gameState == 1 {
        velocity = velocity + gravity * dt
        birdY = birdY + velocity * dt

        if spacePressed {
            velocity = jumpImpulse
        }

        if birdY >= ceilingY {
            birdY = ceilingY
            velocity = 0.0
        }

        var groundHit = birdY - birdRadius <= groundY
        if groundHit {
            birdY = groundY + birdRadius
            GameOver()
            return
        }

        var pipeCount = len(pipes)
        var i = 0
        for i < pipeCount {
            pipes[i].X = pipes[i].X - pipeSpeed * dt

            if pipes[i].X < birdX && pipes[i].Passed == 0 {
                pipes[i].Passed = 1
                score = score + 1
                if score > highScore {
                    highScore = score
                }
                groot.SetScoreDisplay(score, highScore)
                groot.Log(fmt.Sprintf("Score: %d | High: %d", score, highScore))
            }

            var pX = pipes[i].X
            var halfWidth = pipeWidth / 2.0

            var hitRight = birdX + birdRadius > pX - halfWidth
            var hitLeft = birdX - birdRadius < pX + halfWidth
            if hitRight && hitLeft {
                var topPipeBottom = pipes[i].GapY + (pipes[i].GapSize / 2.0)
                var bottomPipeTop = pipes[i].GapY - (pipes[i].GapSize / 2.0)
                var hitTop = birdY + birdRadius > topPipeBottom
                var hitBottom = birdY - birdRadius < bottomPipeTop
                if hitTop || hitBottom {
                    GameOver()
                    return
                }
            }

            var idx = i
            var px = pipes[i].X
            var gy = pipes[i].GapY
            var gs = pipes[i].GapSize
            groot.SetPipePosition(idx, px, gy, gs)

            i = i + 1
        }

        if pipeCount > 0 && pipes[0].X < -350.0 {
            var lastIdx = len(pipes) - 1
            var lastPipeX = pipes[lastIdx].X
            SpawnPipe(lastPipeX + 200.0)

            var newPipes = []Pipe{}
            var k = 1
            for k < len(pipes) {
                newPipes = append(newPipes, pipes[k])
                k = k + 1
            }
            pipes = newPipes
        }

        groot.SetPosition(birdX, birdY)
        return
    }

    if gameState == 2 {
        var onGround = birdY - birdRadius <= groundY
        if !onGround {
            velocity = velocity + gravity * dt
            birdY = birdY + velocity * dt
            var belowGround = birdY - birdRadius < groundY
            if belowGround {
                birdY = groundY + birdRadius
            }
        }

        groot.SetPosition(birdX, birdY)

        if spacePressed {
            ResetGame()
        }
    }
}

func GameOver() {
    gameState = 2
    groot.Warn(fmt.Sprintf("GAME OVER! Score: %d", score))
}
