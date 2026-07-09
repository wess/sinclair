// Dashboard runtime — answers web-view `invoke()` calls that aren't built-in
// app capabilities. Sinclair spawns this once per message.
//
// Protocol: read one JSON request on stdin, write one JSON response on stdout.
//   request:  { kind: "message", panel, method, params?, cwd? }
//   response: { result?: any }   // `result` resolves the page's invoke() promise

const req = JSON.parse((await Bun.stdin.text()) || "{}");

function respond(result: unknown) {
  console.log(JSON.stringify({ result }));
}

if (req.kind === "message" && req.method === "ping") {
  respond({
    pong: true,
    echoedAt: req.params?.at ?? null,
    cwd: req.cwd ?? null,
  });
} else {
  // Unknown method — reply with null so the promise still resolves.
  respond(null);
}
