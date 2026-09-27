import assert from 'node:assert/strict'
import { test } from 'node:test'
import { encodeQr } from '../src/utils/qrcode.ts'

// 对照码：segno 1.6.6 生成（同样的纠错等级、同样的掩模、字节模式、UTF-8、不自动提高纠错等级）。
// segno 在数据刚好落在字节边界时会多补一个 0x00 码字，标准（ISO/IEC 18004 7.4.10）不补；
// 生成这些对照码时把它改成了标准的做法。每一行是一行格子，十六进制，末尾补 0 凑满 4 位。
const FIXTURES = [
  { text: 'HELLO', ecl: 'M', mask: 0, version: 1, rows: ["fe73f8", "82b208", "ba22e8", "ba32e8", "baaae8", "826a08", "feabf8", "005800", "aa5090", "a86210", "0668f8", "ad6210", "676aa0", "00b530", "fe1738", "823d80", "bab738", "ba4330", "baa8a8", "826290", "feab38"] },
  { text: 'WIFI:T:WPA;S:我家的WiFi;P:12345678;;', ecl: 'M', mask: 5, version: 3, rows: ["fe68d3f8", "8288c208", "bab88ae8", "bab64ae8", "ba0662e8", "822c3208", "feaaabf8", "00eb9000", "82ad9e70", "19bdac88", "da1ddf40", "d4a2ba88", "9a0f1888", "198d7ba0", "ea873120", "017e1078", "daa3ef58", "f8c150c0", "ea6f2ce8", "b5331898", "8ab99fe0", "008d8898", "fe4f3aa0", "826528d8", "ba1d2fa8", "ba7952c8", "ba4bf1f0", "821ea2e8", "fec9f8c0"] },
  { text: 'WIFI:T:nopass;S:Guest;;', ecl: 'L', mask: 2, version: 2, rows: ["fe2abf8", "82a3a08", "ba0aae8", "baef2e8", "ba41ae8", "82db208", "feaabf8", "0072000", "fbd6d50", "402a508", "0ea5bf8", "0d0a398", "efefec0", "d543968", "a79ba98", "a1f07d0", "8214fb8", "008e8f8", "fe84af8", "826b890", "ba8df98", "ba86e98", "bafd548", "82b2ac8", "fef84b8"] },
  { text: 'x'.repeat(120), ecl: 'M', mask: 6, version: 7, rows: ["febe8c3bcbf8", "8286198d1208", "bacf3ae0d2e8", "ba22f43bdae8", "ba999ff2bae8", "822988e0c208", "feaaaaaaabf8", "002f18d82000", "9fa1dfca74b8", "38fc216efe90", "67924cd83df8", "f801e7b59320", "afcb396ee690", "49f0d327da00", "e26b8fb58b20", "3d523e910168", "ffccab27c200", "c946884a6cd8", "1ffec6911968", "85752cd825f8", "3feb7fca7fd8", "88d048eef890", "7aa66ad82af8", "48d738b598a0", "9fe76feeff90", "40dae627cf00", "1288953591a0", "60a52b911468", "d272d1a7d880", "3cac1d4a79d8", "6696bc1103e8", "c90d39d830f8", "17c4aaca6e58", "581fd46eeb90", "0bb1ae582778", "78bc62b58620", "9a50efeeff90", "00a648a7c880", "feb7aab58aa0", "82b3189108e8", "baa00fa7cf80", "bae0e7ca6358", "ba3dd1110ee8", "823f83582a78", "fe8ec7ca6340"] },
  { text: 'y'.repeat(300), ecl: 'Q', mask: 1, version: 16, rows: ["fe65c2c20f3295dddc3f8", "822fca559cd6e7fffba08", "ba504f289cac600002ae8", "bae0ed6f4a3538888aae8", "ba65c2fa0732fdddd82e8", "82afca8d94d68ffff5208", "feaaaaaaaaaaaaaaaabf8", "00afb08f7b528fffef800", "621eb3f8b5caff776e340", "459a7cbde0cdea223aba8", "475174aa6b28980001838", "e52f71577d531e3fff950", "e31e5390abcb477776498", "559adcbdeecd6a2222ca0", "7751b4aa7929996001a20", "f52e51577f531ffffff48", "e31fd390b9cbc77770480", "659a1cbde2cc6a2224cb8", "675114aa6d29980005a28", "f52f31577b531ffffdf50", "f31f5390bbcb5f7776490", "659a7cbde4cdea2222cb8", "675114aa6f28980001a28", "c52e31577d430ffffff50", "cf9f53f8b9d3ff7776f90", "789a7c8de4d58a22238b8", "7ad115aa6d38a80001aa8", "f8ae318e7d538fffff8d0", "ffff52f939c2ff7777f90", "7cba7cede4d4ba2222b38", "7a5111c2ed21900080328", "f8ee3450fd5be7feff5d0", "faaf57afb9c26776f6890", "7cea7b6de4c4ba23a2b38", "7a291147ed31900000328", "f8b633b37d73e7fbff5d0", "facf514cb9aa677776890", "7cc27a8be4c4ba2222b38", "7a211721ed79900600328", "f88e3672fd73e7ffff5d0", "fad751cc398a677776890", "74b27e166484ba2222b38", "722110796d79900000328", "fd9637d0fd13e7ffff5d0", "f65f536839ca677776890", "7dba7fee64e4ba2222b38", "77b112656d39900000328", "f40636a0fdf3e7ffff5d0", "ffdf53f839aaff7776f90", "78ba7f8e65258a22228b8", "7ab112ad6d59a80001aa8", "f8863f88fc778fffff8d0", "ffdb4af838adff7776f90", "743a677664269a2223f38", "7a33028d6d5e4008014a8", "fc002fa09cf76feffe4d0", "fedb5a40192c996f76210", "74b87f6e44269c3a23f38", "72331a952cde4400014a8", "f10437b8dcf16dffbe4d0", "f3595a40592a9f7776210", "30be7f6665209a2223f38", "33b512954c5a4000614a8", "710227b8bcf16ffffe4d0", "f3594240192a9f7776210", "70b877660520fa2222738", "73b702956c5a2000014a8", "310237b8bcf12ffffe4d0", "f3594240792ebf7777210", "30b8776665229a2223f18", "73b702954c5c4000014a8", "450237b8b4f16ffffe490", "735942f8692eff7776fb0", "00b8778e75248a22238f8", "fe3742ad5c5ba80000ae8", "8202f688bcf18fffe78d0", "ba5922f8792eff7777f90", "ba3916be6d25e222235a0", "bab6c21d545b6080110b8", "828277e8a6f03effffb40", "fe592340732e169776b08"] },
  { text: 'z'.repeat(20), ecl: 'H', mask: 7, version: 3, rows: ["feb4e3f8", "82df1208", "ba1d7ae8", "bac1aae8", "baa682e8", "82b69a08", "feaaabf8", "005df000", "124579d8", "386fb268", "26bbb250", "bc2e9eb8", "e609c740", "0d701820", "37f52138", "159ae798", "cf914d08", "68269348", "9611b818", "15109dd8", "8b229ff8", "008068e8", "fe5eaad0", "827e28b8", "ba766fc0", "bafb4aa8", "ba512ca8", "8276b790", "fe5f7a10"] },
]

const hexRows = (q) =>
  q.modules.map((row) => {
    const bits = row.map((v) => (v ? '1' : '0')).join('') + '0'.repeat((4 - (row.length % 4)) % 4)
    let hex = ''
    for (let i = 0; i < bits.length; i += 4) hex += parseInt(bits.slice(i, i + 4), 2).toString(16)
    return hex
  })

test('matches the reference codes module by module (versions 1 to 16, all four levels)', () => {
  for (const f of FIXTURES) {
    const q = encodeQr(f.text, { ecl: f.ecl, mask: f.mask })
    assert.equal(q.version, f.version, f.text.slice(0, 20))
    assert.equal(q.size, f.version * 4 + 17)
    assert.deepEqual(hexRows(q), f.rows, `${f.text.slice(0, 20)} ${f.ecl} mask ${f.mask}`)
  }
})

test('picks the smallest version and a mask, and the chosen mask reproduces the same code', () => {
  const q = encodeQr('WIFI:T:WPA;S:我家的WiFi;P:12345678;;')
  assert.equal(q.version, 3, '中文在 UTF-8 里一个字三个字节，一共 39 个字节，版本 3（M 级）刚好放得下')
  assert.ok(q.mask >= 0 && q.mask < 8)
  assert.deepEqual(encodeQr('WIFI:T:WPA;S:我家的WiFi;P:12345678;;', { mask: q.mask }).modules, q.modules)
  assert.throws(() => encodeQr('x'.repeat(3000)), /太长/)
})
