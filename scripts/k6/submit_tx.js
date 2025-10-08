// k6 script to POST a transaction to the node: k6 run scripts/k6/submit_tx.js
import http from 'k6/http';
import { check, sleep } from 'k6';

export const options = {
  vus: 5,
  duration: '10s',
};

export default function () {
  const payload = JSON.stringify({ from: 'alice', to: 'bob', amount: 1 });
  const res = http.post('http://127.0.0.1:8080/tx', payload, {
    headers: { 'Content-Type': 'application/json' },
  });
  check(res, { 'status was 200': (r) => r.status === 200 });
  sleep(1);
}
