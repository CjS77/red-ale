<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/network-disabled
summary: The method needs the network and the Configuration has it off.
---
**Cause.** Fetching online quotes (and other methods that use the network) is off in red-ale's Configuration.
Nothing was sent.

**Fix.** Turn it on with [config.set](method/config.set) if you want it, and call again. The setting is there so a
headless server never reaches the network unasked.
