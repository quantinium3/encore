import { Room } from "./room";

export { Room };

interface Env {
	ROOMS: DurableObjectNamespace;
}

const CODE = /^[A-HJ-NP-Z2-9]{6}$/;

export default {
	async fetch(request: Request, env: Env): Promise<Response> {
		const url = new URL(request.url);
		const match = url.pathname.match(/^\/room\/([^/]+)\/(join|intent|state|leave)$/);
		if (!match) {
			return new Response("not found", { status: 404 });
		}
		const code = match[1].toUpperCase();
		if (!CODE.test(code)) {
			return new Response("invalid room code", { status: 400 });
		}
		const stub = env.ROOMS.get(env.ROOMS.idFromName(code));
		return stub.fetch(request);
	},
};
