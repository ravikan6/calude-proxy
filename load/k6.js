import http from "k6/http";
import { check, sleep } from "k6";

export const options = {
  scenarios: {
    team_gateway: {
      executor: "constant-arrival-rate",
      rate: Number(__ENV.RATE || 200),
      timeUnit: "1s",
      duration: __ENV.DURATION || "30m",
      preAllocatedVUs: 200,
      maxVUs: 500,
    },
  },
  thresholds: {
    http_req_failed: ["rate<0.001"],
    http_req_duration: ["p(95)<100"],
  },
};

const baseUrl = __ENV.PROXY_URL || "http://127.0.0.1:8082";
const key = __ENV.PROXY_CLIENT_KEY;

export default function () {
  const response = http.post(
    `${baseUrl}/v1/messages`,
    JSON.stringify({
      model: __ENV.MODEL || "claude-sonnet-load-test",
      max_tokens: 32,
      messages: [{ role: "user", content: "Reply with ok" }],
    }),
    {
      headers: {
        "content-type": "application/json",
        "anthropic-version": "2023-06-01",
        "x-api-key": key,
      },
    },
  );
  check(response, { "status is 200": (result) => result.status === 200 });
  sleep(0.01);
}
