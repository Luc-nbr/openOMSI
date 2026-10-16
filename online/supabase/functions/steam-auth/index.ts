// Signing in with Steam (see steam.mjs). Deployed with verify_jwt off: the player has no session yet.
import { handle } from './steam.mjs';

const url = Deno.env.get('SUPABASE_URL')!;
const keys = Deno.env.get('SUPABASE_SECRET_KEYS');
const key = (keys && JSON.parse(keys).default) || Deno.env.get('SUPABASE_SERVICE_ROLE_KEY')!;

Deno.serve((req) => handle(req, { url, key }));
