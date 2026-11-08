import { DurableObject } from "cloudflare:workers";

interface Track {
	id: string;
	title: string;
	artist?: string | null;
	artist_id?: string | null;
	album?: string | null;
	duration?: string | null;
	thumbnail_url?: string | null;
	credits?: string[];
}

interface State {
	rev: number;
	tracks: Track[];
	index: number | null;
	playing: boolean;
	position_secs: number;
	updated_at_ms: number;
	repeat: boolean;
}

type Intent =
	| { type: "play_queue"; tracks: Track[]; start: number }
	| { type: "play" }
	| { type: "pause" }
	| { type: "seek"; position_secs: number }
	| { type: "next"; expected_index?: number | null }
	| { type: "previous" }
	| { type: "append"; tracks: Track[]; expected_len: number }
	| { type: "set_repeat"; repeat: boolean };

const MAX_TRACKS = 500;
const MAX_BODY = 512 * 1024;
const POLL_MS = 25_000;
const MEMBER_TTL_MS = 45_000;
const IDLE_MS = 6 * 60 * 60 * 1000;
const RESTART_THRESHOLD_SECS = 3;

export class Room extends DurableObject {
	private state: State = {
		rev: 0,
		tracks: [],
		index: null,
		playing: false,
		position_secs: 0,
		updated_at_ms: Date.now(),
		repeat: false,
	};
	private members = new Map<string, number>();
	private waiters: Array<() => void> = [];
	private loaded: Promise<void>;

	constructor(ctx: DurableObjectState, env: unknown) {
		super(ctx, env as never);
		this.loaded = ctx.blockConcurrencyWhile(async () => {
			const saved = await ctx.storage.get<State>("state");
			if (saved) {
				this.state = saved;
			}
		});
	}

	async fetch(request: Request): Promise<Response> {
		await this.loaded;
		const url = new URL(request.url);
		const action = url.pathname.split("/").pop();
		const client = url.searchParams.get("client") ?? "";
		if (client.length === 0 || client.length > 64) {
			return json({ error: "missing client id" }, 400);
		}
		this.touch(client);

		switch (action) {
			case "join":
				return request.method === "POST" ? this.snapshot() : json({ error: "method" }, 405);
			case "leave":
				this.members.delete(client);
				this.wake();
				return json({ ok: true });
			case "state":
				return this.longPoll(Number(url.searchParams.get("since") ?? "0"));
			case "intent":
				return request.method === "POST" ? this.intent(request) : json({ error: "method" }, 405);
		}
		return json({ error: "not found" }, 404);
	}

	async alarm(): Promise<void> {
		this.pruneMembers();
		if (this.members.size === 0 && Date.now() - this.state.updated_at_ms > IDLE_MS) {
			await this.ctx.storage.deleteAll();
			return;
		}
		await this.ctx.storage.setAlarm(Date.now() + IDLE_MS / 4);
	}

	private touch(client: string): void {
		const isNew = !this.members.has(client);
		this.members.set(client, Date.now());
		if (isNew) {
			this.wake();
		}
		void this.ctx.storage.getAlarm().then((alarm) => {
			if (alarm === null) {
				return this.ctx.storage.setAlarm(Date.now() + IDLE_MS / 4);
			}
		});
	}

	private pruneMembers(): void {
		const cutoff = Date.now() - MEMBER_TTL_MS;
		for (const [id, seen] of this.members) {
			if (seen < cutoff) {
				this.members.delete(id);
			}
		}
	}

	private snapshot(): Response {
		this.pruneMembers();
		return json({
			state: this.state,
			members: this.members.size,
			server_now_ms: Date.now(),
		});
	}

	private async longPoll(since: number): Promise<Response> {
		if (this.state.rev > since) {
			return this.snapshot();
		}
		await new Promise<void>((resolve) => {
			const timer = setTimeout(done, POLL_MS);
			const waiter = () => done();
			function done() {
				clearTimeout(timer);
				resolve();
			}
			this.waiters.push(waiter);
		});
		return this.snapshot();
	}

	private wake(): void {
		const waiters = this.waiters;
		this.waiters = [];
		for (const waiter of waiters) {
			waiter();
		}
	}

	private async intent(request: Request): Promise<Response> {
		const text = await request.text();
		if (text.length > MAX_BODY) {
			return json({ error: "too large" }, 413);
		}
		let intent: Intent;
		try {
			intent = JSON.parse(text) as Intent;
		} catch {
			return json({ error: "bad json" }, 400);
		}
		if (this.apply(intent)) {
			this.state.rev += 1;
			await this.ctx.storage.put("state", this.state);
			this.wake();
		}
		return this.snapshot();
	}

	private position(now: number): number {
		const s = this.state;
		return s.playing ? s.position_secs + (now - s.updated_at_ms) / 1000 : s.position_secs;
	}

	private apply(intent: Intent): boolean {
		const s = this.state;
		const now = Date.now();
		switch (intent.type) {
			case "play_queue": {
				const tracks = sanitize(intent.tracks);
				if (tracks.length === 0 || !validIndex(intent.start, tracks.length)) {
					return false;
				}
				s.tracks = tracks;
				s.index = intent.start;
				s.playing = true;
				s.position_secs = 0;
				s.updated_at_ms = now;
				return true;
			}
			case "play":
				if (s.index === null || s.playing) {
					return false;
				}
				s.playing = true;
				s.updated_at_ms = now;
				return true;
			case "pause":
				if (s.index === null || !s.playing) {
					return false;
				}
				s.position_secs = this.position(now);
				s.playing = false;
				s.updated_at_ms = now;
				return true;
			case "seek":
				if (s.index === null || !Number.isFinite(intent.position_secs)) {
					return false;
				}
				s.position_secs = Math.max(0, intent.position_secs);
				s.updated_at_ms = now;
				return true;
			case "next": {
				if (s.index === null) {
					return false;
				}
				if (intent.expected_index != null && intent.expected_index !== s.index) {
					return false;
				}
				if (s.index + 1 < s.tracks.length) {
					s.index += 1;
				} else if (s.repeat) {
					s.index = 0;
				} else {
					s.playing = false;
					s.position_secs = 0;
					s.updated_at_ms = now;
					return true;
				}
				s.playing = true;
				s.position_secs = 0;
				s.updated_at_ms = now;
				return true;
			}
			case "previous": {
				if (s.index === null) {
					return false;
				}
				if (this.position(now) > RESTART_THRESHOLD_SECS || s.index === 0) {
					s.position_secs = 0;
				} else {
					s.index -= 1;
					s.position_secs = 0;
				}
				s.playing = true;
				s.updated_at_ms = now;
				return true;
			}
			case "append": {
				if (intent.expected_len !== s.tracks.length) {
					return false;
				}
				const known = new Set(s.tracks.map((t) => t.id));
				const added = sanitize(intent.tracks)
					.filter((t) => !known.has(t.id) && known.add(t.id))
					.slice(0, MAX_TRACKS - s.tracks.length);
				if (added.length === 0) {
					return false;
				}
				s.tracks = s.tracks.concat(added);
				return true;
			}
			case "set_repeat":
				if (s.repeat === !!intent.repeat) {
					return false;
				}
				s.repeat = !!intent.repeat;
				return true;
		}
		return false;
	}
}

function sanitize(tracks: unknown): Track[] {
	if (!Array.isArray(tracks)) {
		return [];
	}
	return tracks
		.filter((t): t is Track => !!t && typeof t.id === "string" && typeof t.title === "string")
		.slice(0, MAX_TRACKS);
}

function validIndex(index: unknown, len: number): index is number {
	return Number.isInteger(index) && (index as number) >= 0 && (index as number) < len;
}

function json(body: unknown, status = 200): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { "content-type": "application/json" },
	});
}
