package uc2024

import (
	"fmt"
	"math/rand"
	"net/http"
	"regexp"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
)

type PlayerTeam string

const (
	PlayerTeamWhite PlayerTeam = "white"
	PlayerTeamBlack PlayerTeam = "black"
)

type ActiveGame struct {
	moves            []string
	gameOver         bool
	lastReceivedTime time.Time
	startTime        time.Time
	playerIps        map[string]PlayerTeam
	host             string
	chessVariant     string
}

var accessLock *sync.Mutex = &sync.Mutex{}
var activeGames map[string]ActiveGame = make(map[string]ActiveGame)

func init() {
	go purgeInactiveGames()
}

func getGame(c *gin.Context) {
	gameKey := c.Param("game_key")

	accessLock.Lock()
	defer accessLock.Unlock()
	game, ok := activeGames[gameKey]
	if !ok {
		time.Sleep(5 * time.Second)
		c.JSON(http.StatusNotFound, gin.H{
			"error": "game not found",
		})
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"moves":         game.moves,
		"game_ready":    len(game.playerIps) == 2,
		"host_team":     game.playerIps[game.host],
		"game_complete": game.gameOver,
	})
}

func postMove(c *gin.Context) {
	gameKey := c.Param("game_key")
	move := c.Query("move")
	if len(move) > 20 {
		c.JSON(http.StatusForbidden, gin.H{
			"error": "move too long",
		})
		return
	}

	accessLock.Lock()
	defer accessLock.Unlock()
	game, ok := activeGames[gameKey]
	if !ok {
		time.Sleep(5 * time.Second)
		c.JSON(http.StatusNotFound, gin.H{
			"error": "game not found",
		})
		return
	}

	if game.gameOver {
		c.JSON(http.StatusForbidden, gin.H{
			"error": "game already over",
		})
		return
	}

	if len(game.moves) > 500 {
		c.JSON(http.StatusForbidden, gin.H{
			"error": "max moves hit",
		})
		return
	}

	game.moves = append(game.moves, move)
	game.lastReceivedTime = time.Now()
	activeGames[gameKey] = game

	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
	})
}

func generateGameKey() string {
	possibleKeyChars := []rune("abcdefghjkmnrstuvwxyz34678")
	gameKey := ""
	for i := 0; i < 6; i++ {
		gameKey += string(possibleKeyChars[rand.Intn(len(possibleKeyChars))])
	}

	return gameKey
}

func getPlayerKey(c *gin.Context) string {
	return c.Query("player_key")
}

func checkPlayerKey(c *gin.Context) bool {
	return len(getPlayerKey(c)) <= 0 || len(getPlayerKey(c)) > 20
}

func postCreateGame(c *gin.Context) {
	if !checkPlayerKey(c) {
		c.JSON(http.StatusBadRequest, gin.H{
			"error": "invalid player key",
		})
		return
	}

	chessVariant := c.Query("chess_variant")
	validPattern := "^(Chess960\\(\\d{0,10}\\))|(Standard)|(Horde)|(Horsies)|(Kawns)$"
	re := regexp.MustCompile(validPattern)
	if !re.Match([]byte(chessVariant)) {
		fmt.Printf("Invalid chess variant: %s\n", chessVariant)
		c.JSON(http.StatusBadRequest, gin.H{
			"error": "invalid chess variant",
		})
		return
	}

	gameKey := generateGameKey()

	accessLock.Lock()
	defer accessLock.Unlock()
	if len(activeGames) > 100 {
		c.JSON(http.StatusTooManyRequests, gin.H{
			"error": "too many active games",
		})
		return
	}

	var team PlayerTeam
	if rand.Int()%2 == 0 {
		team = PlayerTeamWhite
	} else {
		team = PlayerTeamBlack
	}

	activeGames[gameKey] = ActiveGame{
		moves:            []string{},
		startTime:        time.Now(),
		lastReceivedTime: time.Now(),
		host:             getPlayerKey(c),
		playerIps: map[string]PlayerTeam{
			getPlayerKey(c): team,
		},
		chessVariant: chessVariant,
	}

	c.JSON(http.StatusOK, gin.H{
		"game_key": gameKey,
	})
}

func postJoinGame(c *gin.Context) {
	if !checkPlayerKey(c) {
		c.JSON(http.StatusForbidden, gin.H{
			"error": "invalid player key",
		})
		return
	}

	gameKey := c.Param("game_key")

	accessLock.Lock()
	defer accessLock.Unlock()
	game, ok := activeGames[gameKey]
	if !ok {
		time.Sleep(5 * time.Second)
		c.JSON(http.StatusNotFound, gin.H{
			"error": "game not found",
		})
		return
	}

	if len(game.playerIps) >= 2 {
		c.JSON(http.StatusForbidden, gin.H{
			"error": "game already full",
		})
		return
	}

	var team PlayerTeam
	if game.playerIps[game.host] == PlayerTeamWhite {
		team = PlayerTeamBlack
	} else {
		team = PlayerTeamWhite
	}

	game.playerIps[getPlayerKey(c)] = team
	activeGames[gameKey] = game

	c.JSON(http.StatusOK, gin.H{
		"game_key":      gameKey,
		"host":          game.playerIps[game.host],
		"chess_variant": game.chessVariant,
	})
}

func purgeInactiveGames() {
	for {
		time.Sleep(1 * time.Minute)
		accessLock.Lock()
		for key, game := range activeGames {
			if time.Since(game.lastReceivedTime) > 10*time.Minute || time.Since(game.startTime) > 1*time.Hour {
				delete(activeGames, key)
			}
		}
		accessLock.Unlock()
	}
}

func deleteGame(c *gin.Context) {
	gameKey := c.Param("game_key")

	accessLock.Lock()
	defer accessLock.Unlock()
	_, ok := activeGames[gameKey]
	if !ok {
		time.Sleep(5 * time.Second)
		c.JSON(http.StatusNotFound, gin.H{
			"error": "game not found",
		})
		return
	}

	delete(activeGames, gameKey)

	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
	})
}

func AddChessServerGroup(r *gin.Engine) {
	group := r.Group("/uc2024")
	group.POST("/create", postCreateGame)
	group.POST("/join/:game_key", postJoinGame)
	group.POST("/move/:game_key", postMove)
	group.GET("/game/:game_key", getGame)
	group.DELETE("/game/:game_key", deleteGame)
}
