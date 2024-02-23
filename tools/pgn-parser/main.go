package main

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"math"
	"math/bits"
	"os"
	"strings"
	"sync"
	"time"
	"unicode"

	"gopkg.in/freeeve/pgn.v1"
)

type PieceValueTableInput struct {
	Pawn   float32 `json:"pawn"`
	Knight float32 `json:"knight"`
	Bishop float32 `json:"bishop"`
	Rook   float32 `json:"rook"`
	Queen  float32 `json:"queen"`
}

type GenerateInput struct {
	PlayerName        string                `json:"name"`
	FileName          string                `json:"file"`
	Depth             PlayerAIThinkingDepth `json:"depth"`
	PieceValueTable   PieceValueTableInput  `json:"piece_values"`
	CheckBonus        float32               `json:"check_bonus"`
	DecisionAlgorithm string                `json:"decision_algorithm"`
}

type PgnMove struct {
	M string `json:"m"`
}

type PgnGame struct {
	White   string    `json:"White"`
	Black   string    `json:"Black"`
	Variant string    `json:"Variant"`
	Moves   []PgnMove `json:"moves"`
}

type PlayerAITeamProfile struct {
	Positions map[string]map[string]int `json:"positions"`
}

type PieceSquareTables struct {
	Pawn   [64]int `json:"pawn"`
	Knight [64]int `json:"knight"`
	Bishop [64]int `json:"bishop"`
	Rook   [64]int `json:"rook"`
	Queen  [64]int `json:"queen"`
	King   [64]int `json:"king"`
}

func PieceSquareTableNew(input map[string][64]int) PieceSquareTables {
	return PieceSquareTables{
		Pawn:   input["p"],
		Knight: input["n"],
		Bishop: input["b"],
		Rook:   input["r"],
		Queen:  input["q"],
		King:   input["k"],
	}
}

type PieceSquarePhases struct {
	Opening    PieceSquareTables `json:"opening"`
	MiddleGame PieceSquareTables `json:"middle_game"`
	EndGame    PieceSquareTables `json:"end_game"`
}

type PlayerAIThinkingDepth struct {
	Depth        []int     `json:"levels"`
	MoveHit      []float32 `json:"move_hit"`
	ThinkingTime []float32 `json:"thinking_time"`
}

type PlayerAIProfile struct {
	White             PlayerAITeamProfile   `json:"white"`
	Black             PlayerAITeamProfile   `json:"black"`
	Depth             PlayerAIThinkingDepth `json:"depth"`
	PieceWeights      []float32             `json:"piece_weights"`
	PiecePhaseTable   PieceSquarePhases     `json:"piece_square_phases"`
	CheckBonus        float32               `json:"check_bonus"`
	DecisionAlgorithm string                `json:"decision_algorithm"`
}

type PlayerAIGroup struct {
	Profiles map[string]PlayerAIProfile `json:"profiles"`
}

type GamePhase string

const (
	Opening    GamePhase = "opening"
	MiddleGame GamePhase = "middle_game"
	EndGame    GamePhase = "end_game"
)

func GetGamePhase(board *pgn.Board) GamePhase {
	fen := board.String()
	// Counting the number of minor pieces (Bishops and Knights), major pieces (Rooks and Queens), and pawns.
	minorPieces := strings.Count(fen, "b") + strings.Count(fen, "n") + strings.Count(fen, "B") + strings.Count(fen, "N")
	majorPieces := strings.Count(fen, "r") + strings.Count(fen, "q") + strings.Count(fen, "R") + strings.Count(fen, "Q")
	pawns := strings.Count(fen, "p") + strings.Count(fen, "P")

	// Simple heuristic to determine the game phase
	if pawns > 14 && minorPieces == 4 && majorPieces >= 4 {
		return Opening
	} else if pawns <= 14 && minorPieces <= 4 && majorPieces <= 4 {
		return EndGame
	}

	return MiddleGame
}

func hash(s string) string {
	var h uint32
	for i := 0; i < len(s); i++ {
		h = h + uint32(s[i])
		h = h + (h << 10)
		h = h ^ (h >> 6)
	}

	h = h + (h << 3)
	h = h ^ (h >> 11)
	h = h + (h << 15)

	data := []byte{byte(h >> 24), byte(h >> 16), byte(h >> 8), byte(h)}

	return base64.StdEncoding.EncodeToString(data)[:5]
}

const pieces = "pnbrqk"

func generatePieceCountString(fen string, team pgn.Color) string {
	piece_map := map[string]int{}
	for _, piece := range pieces {
		piece_map[string(piece)] = 0
	}

	for _, c := range fen {
		var piece string
		if unicode.IsUpper(c) && team == pgn.White {
			piece = strings.ToLower(string(c))
		} else if unicode.IsLower(c) && team == pgn.Black {
			piece = strings.ToLower(string(c))
		}

		if _, ok := piece_map[piece]; ok {
			piece_map[piece]++
		}
	}

	result := ""
	for i, piece := range pieces {
		result += fmt.Sprintf("%d", piece_map[string(piece)])
		if i < len(pieces)-1 {
			result += ","
		}
	}

	return result
}

func pieceMoved(move string) string {
	if strings.Contains(move, "O") {
		return "k"
	}

	if strings.Contains(pieces, strings.ToLower(string(move[0]))) {
		return strings.ToLower(string(move[0]))
	}

	return "p"
}

func convertToPercentages(toUpdate map[string]map[string]int) map[string]map[string]int {
	for key, positionCount := range toUpdate {
		total := float32(0)
		for _, count := range positionCount {
			total += float32(count)
		}

		for move, count := range positionCount {
			toUpdate[key][move] = int(float32(count) / total * 100)
		}

		toUpdate[key] = positionCount
	}

	return toUpdate
}

func SwitchTurn(current pgn.Color) pgn.Color {
	if current == pgn.White {
		return pgn.Black
	} else {
		return pgn.White
	}
}

func (g *GenerateInput) GenerateProfile() PlayerAIProfile {
	fileName := g.FileName
	playerName := g.PlayerName

	totalUniqueGameStates := map[string]bool{}
	totalGameStates := 0

	pieceSquareCounts := map[GamePhase]map[string][64]int{}
	for _, phase := range []GamePhase{Opening, MiddleGame, EndGame} {
		pieceSquareCounts[phase] = map[string][64]int{}
	}

	var games []PgnGame

	{
		data, _ := os.ReadFile(fileName)
		json.Unmarshal(data, &games)
	}

	player := PlayerAIProfile{
		White: PlayerAITeamProfile{
			Positions: map[string]map[string]int{},
		},
		Black: PlayerAITeamProfile{
			Positions: map[string]map[string]int{},
		},
	}

	for _, game := range games {
		var playerProfile *PlayerAITeamProfile
		var playerTeam pgn.Color
		if game.White == playerName {
			playerTeam = pgn.White
			playerProfile = &player.White
		} else {
			playerTeam = pgn.Black
			playerProfile = &player.Black
		}

		if game.Variant != "Standard" && game.Variant != "" {
			continue
		}

		currentTurn := pgn.White
		b := pgn.NewBoard()
		for i := 0; i < len(game.Moves); i++ {
			// Gen FEN
			gameState := b.String()

			parsedMove, err := b.MoveFromAlgebraic(game.Moves[i].M, currentTurn)
			if err != nil {
				// fmt.Printf("Game:%v Error parsing move: %s\n", game, err)
				break
			}

			b.MakeMove(parsedMove)

			if currentTurn != playerTeam {
				currentTurn = SwitchTurn(currentTurn)
				continue
			}

			// Remove Move and half move number
			splits := strings.Split(gameState, " ")
			gameState = splits[0]
			positionHash := hash(gameState)

			totalUniqueGameStates[positionHash] = true
			totalGameStates++

			move := game.Moves[i].M

			if i < 10 {
				// Get next move and add to position map
				if _, ok := playerProfile.Positions[positionHash]; !ok {
					playerProfile.Positions[positionHash] = map[string]int{}
				}
				playerProfile.Positions[positionHash][move]++
			}

			// Only update tables when queens are moved
			if strings.Contains(gameState, "Q") || strings.Contains(gameState, "q") {
				// Update piece square tables
				index := bits.TrailingZeros(uint(parsedMove.To))
				// Flip index if black
				if currentTurn == pgn.Black {
					index = 63 - index
				}
				key := pieceMoved(move)

				phase := GetGamePhase(b)
				phaseTable := pieceSquareCounts[phase]
				pieceTable := phaseTable[key]
				pieceTable[index]++
				phaseTable[key] = pieceTable
				pieceSquareCounts[phase] = phaseTable
			}

			currentTurn = SwitchTurn(currentTurn)
		}
	}

	player.White.Positions = convertToPercentages(player.White.Positions)
	player.Black.Positions = convertToPercentages(player.Black.Positions)

	// Convert piece square tables
	for _, phase := range []GamePhase{Opening, MiddleGame, EndGame} {
		phaseTable := pieceSquareCounts[phase]
		for _, piece := range pieces {
			sum := 0.0
			values := phaseTable[string(piece)]
			for _, count := range values {
				sum += float64(count)
			}

			if sum > 0 {
				for i, count := range values {
					values[i] = int(math.Ceil((float64(count) / sum * 100.0)))
				}
			}

			// sanity check print board with percentages
			// fmt.Printf("---------------------- %c ----------------------\n", piece)
			// fmt.Printf("Phase %s Piece: %c Sum: %f\n", phase, piece, sum)
			// for rank := 0; rank < 8; rank++ {
			// 	if rank == 0 {
			// 		fmt.Printf("   ")
			// 		for file := 0; file < 8; file++ {
			// 			fmt.Printf("%c    ", 'A'+file)
			// 		}
			// 		fmt.Printf("\n")
			// 	}
			// 	fmt.Printf("%d ", 8-rank)
			// 	for file := 0; file < 8; file++ {
			// 		index := (7-rank)*8 + file
			// 		if values[index] == 0 {
			// 			fmt.Printf("---- ")
			// 			continue
			// 		} else {
			// 			fmt.Printf("%04d ", values[index])
			// 		}
			// 	}
			// 	fmt.Printf("\n")
			// }

			phaseTable[string(piece)] = values
		}

		pieceSquareCounts[phase] = phaseTable
	}

	player.PiecePhaseTable = PieceSquarePhases{
		Opening:    PieceSquareTableNew(pieceSquareCounts[Opening]),
		MiddleGame: PieceSquareTableNew(pieceSquareCounts[MiddleGame]),
		EndGame:    PieceSquareTableNew(pieceSquareCounts[EndGame]),
	}

	// Convert piece value table
	player.PieceWeights = []float32{
		float32(g.PieceValueTable.Pawn),
		float32(g.PieceValueTable.Knight),
		float32(g.PieceValueTable.Bishop),
		float32(g.PieceValueTable.Rook),
		float32(g.PieceValueTable.Queen),
		// King always worth 200
		200.,
	}

	player.CheckBonus = g.CheckBonus
	player.DecisionAlgorithm = g.DecisionAlgorithm

	fmt.Printf("Player: %s UGS:%d TGS:%d\n", playerName, len(totalUniqueGameStates), totalGameStates)

	return player
}

func main() {
	var generateProfiles []GenerateInput
	{
		data, err := os.ReadFile("generate.json")
		if err != nil {
			fmt.Println(err)
			os.Exit(1)
		}
		if err := json.Unmarshal(data, &generateProfiles); err != nil {
			fmt.Println(err)
			os.Exit(1)
		}
	}

	output := PlayerAIGroup{
		Profiles: map[string]PlayerAIProfile{},
	}
	for _, g := range generateProfiles {
		profile := g.GenerateProfile()
		profile.Depth = g.Depth
		output.Profiles[g.PlayerName] = profile
	}

	{
		jsonBytes, _ := json.Marshal(output)
		jsonString := string(jsonBytes)

		os.WriteFile("player_profiles.computer.json", []byte(jsonString), 0644)
	}
}

func example() {
	jobs := []string{"a", "b", "c", "d", "e", "f", "g", "h"}

	results := make(chan string, len(jobs))

	wg := &sync.WaitGroup{}

	for _, job := range jobs {
		wg.Add(1)
		go func() {
			defer wg.Done()
			fmt.Println(job)
			time.Sleep(1 * time.Second)
			results <- job
		}()
	}

	wg.Wait()

	close(results)

	for result := range results {
		fmt.Println(result)
	}

}
