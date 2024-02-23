package main

import (
	"github.com/sardap/ultimate-chess-2024/server/uc2024"

	"github.com/gin-gonic/gin"
)

func main() {
	r := gin.Default()

	uc2024.AddChessServerGroup(r)

	r.Run(":8543") // listen and serve on 0.0.0.0:8080 (for windows "localhost:8080")
}
