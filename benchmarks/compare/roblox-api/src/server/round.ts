import { Players, ReplicatedStorage, Teams } from "@rbxts/services";
import { GamePhase, INTERMISSION_LENGTH, ROUND_LENGTH, StatName, TEAM_COLORS } from "../shared/constants";
import { Signal } from "../shared/signal";
import { describeStandings, incrementStat, rankPlayers, resetStats } from "./leaderboard";

export class RoundController {
	public readonly phaseChanged = new Signal<[phase: GamePhase, remaining: number]>();
	private phase = GamePhase.Waiting;
	private remaining = 0;
	private readonly state: Folder;
	private readonly teams = new Array<Team>();
	private running = false;

	public constructor(private readonly minimumPlayers: number) {
		this.state = new Instance("Folder");
		this.state.Name = "RoundState";
		this.state.Parent = ReplicatedStorage;
		TEAM_COLORS.forEach((color, index) => {
			const team = new Instance("Team");
			team.Name = `Team ${index + 1}`;
			team.TeamColor = new BrickColor(color);
			team.AutoAssignable = true;
			team.Parent = Teams;
			this.teams.push(team);
		});
	}

	public getPhase(): GamePhase {
		return this.phase;
	}

	private setPhase(phase: GamePhase, duration: number): void {
		this.phase = phase;
		this.remaining = duration;
		this.state.SetAttribute("Phase", phase);
		this.state.SetAttribute("Remaining", duration);
		this.phaseChanged.fire(phase, duration);
	}

	private countdown(): void {
		while (this.remaining > 0 && this.running) {
			task.wait(1);
			this.remaining--;
			this.state.SetAttribute("Remaining", this.remaining);
		}
	}

	private balanceTeams(): void {
		const players = Players.GetPlayers();
		players.sort((left, right) => left.UserId < right.UserId);
		players.forEach((player, index) => {
			player.Team = this.teams[index % this.teams.size()];
			player.LoadCharacter();
		});
	}

	private awardWinner(): Player | undefined {
		const [first] = rankPlayers(StatName.Coins);
		if (!first) return undefined;
		const [winner] = first;
		incrementStat(winner, StatName.Wins);
		return winner;
	}

	public start(): void {
		if (this.running) return;
		this.running = true;
		task.spawn(() => {
			while (this.running) {
				if (Players.GetPlayers().size() < this.minimumPlayers) {
					this.setPhase(GamePhase.Waiting, 0);
					task.wait(2);
					continue;
				}
				this.setPhase(GamePhase.Intermission, INTERMISSION_LENGTH);
				this.countdown();
				for (const player of Players.GetPlayers()) resetStats(player);
				this.balanceTeams();
				this.setPhase(GamePhase.Playing, ROUND_LENGTH);
				this.countdown();
				const winner = this.awardWinner();
				this.state.SetAttribute("Winner", winner?.DisplayName ?? "nobody");
				this.state.SetAttribute("Standings", describeStandings(StatName.Coins, 5));
				this.setPhase(GamePhase.Results, 8);
				this.countdown();
			}
		});
	}

	public stop(): void {
		this.running = false;
	}
}
