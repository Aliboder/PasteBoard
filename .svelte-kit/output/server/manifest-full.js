export const manifest = (() => {
function __memo(fn) {
	let value;
	return () => value ??= (value = fn());
}

return {
	appDir: "_app",
	appPath: "_app",
	assets: new Set(["favicon.png","svelte.svg","tauri.svg","vite.svg"]),
	mimeTypes: {".png":"image/png",".svg":"image/svg+xml"},
	_: {
		client: {start:"_app/immutable/entry/start.fcEKSt9Q.js",app:"_app/immutable/entry/app.DCMJ4VAN.js",imports:["_app/immutable/entry/start.fcEKSt9Q.js","_app/immutable/chunks/BRiWr7pi.js","_app/immutable/chunks/C1cskHmE.js","_app/immutable/chunks/Bg2Kr_2r.js","_app/immutable/entry/app.DCMJ4VAN.js","_app/immutable/chunks/C1cskHmE.js","_app/immutable/chunks/DPwJXdwf.js","_app/immutable/chunks/DHXLQuD7.js","_app/immutable/chunks/Bg2Kr_2r.js","_app/immutable/chunks/DpiBmUOu.js"],stylesheets:[],fonts:[],uses_env_dynamic_public:false},
		nodes: [
			__memo(() => import('./nodes/0.js')),
			__memo(() => import('./nodes/1.js')),
			__memo(() => import('./nodes/2.js'))
		],
		remotes: {
			
		},
		routes: [
			{
				id: "/",
				pattern: /^\/$/,
				params: [],
				page: { layouts: [0,], errors: [1,], leaf: 2 },
				endpoint: null
			}
		],
		prerendered_routes: new Set([]),
		matchers: async () => {
			
			return {  };
		},
		server_assets: {}
	}
}
})();
