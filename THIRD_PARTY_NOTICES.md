# Third-party notices

The frontend links SDL3 (zlib), FFmpeg and XDeviceBridge's client library
(LGPL-3.0-only). The FFmpeg build configuration determines its LGPL/GPL
requirements. Cargo.lock records the Rust dependencies.

The Apple DCT decoder includes Rust code ported from stb_image under the MIT
license. The stb license is installed under `share/licenses/stb`.

XDeviceBridge LICENSE and COPYING.GPLv3 are installed under
`share/licenses/XDeviceBridge`. Its fixed source tree is included in the matching
source-materials archive. Other runtime licenses, source versions and build
recipes are recorded by the platform packaging tools.

Device protocols, OpenSSL, libcurl, plist, lwIP, mDNS, DDI and live media decoding
belong to the separately installed XDeviceBridge Server. Their notices and source
materials are maintained by that project. DDI payloads are downloaded separately.
