Live Scroll mainnet GalileoV2 fixtures.

Source range:
- `32144474`
- `32144475`
- `32144476`

Dumped from `https://scroll.api.onfinality.io/public` with DogeOS SBV commit `9a6c90f`,
using `sbv-cli dump` plus the authenticated `L2MessageQueue` proof augmentation.

The public endpoint rate-limited concurrent dumps, so these were generated as single-block
dumps with:

```bash
sbv-cli dump \
  --block <BLOCK> \
  --rpc https://scroll.api.onfinality.io/public \
  --requests-per-second 1 \
  --compute-units-per-second 50 \
  --max-rate-limit-retries 20 \
  --initial-backoff 500
```
