// Syntax highlighting for ```slab blocks: registers a language with the highlight.js that mdBook
// ships, and highlights the blocks again. The keyword list is checked against the compiler's own
// (`tests/docs_examples.rs`), so a new keyword cannot be forgotten.
(function () {
  if (typeof hljs === 'undefined') return;
  hljs.registerLanguage('slab', function (hljs) {
    return {
      name: 'slab',
      keywords: {
        keyword: 'fn let mut if else while loop for in return break continue struct enum impl match global const objective as at be positioned rotated facing align anchored pub import task start',
        literal: 'true false',
        built_in: 'int bool str list Entity Option Result Some None Ok Err NearDouble FarDouble print say tp kill exists summon setblock particle run_command',
      },
      contains: [
        { className: 'comment', begin: '//[/!]?', end: '$' },
        hljs.C_BLOCK_COMMENT_MODE,
        { className: 'string', begin: 'f?"', end: '"', contains: [{ begin: '\\\\.' }] },
        { className: 'meta', begin: '#\\[', end: '\\]' },
        { className: 'symbol', begin: '@[aeprsn]\\b(\\[[^\\]]*\\])?' },
        { className: 'number', begin: '\\b\\d[\\d_]*(\\.\\d+)?[tsd]?\\b' },
        { className: 'variable', begin: '[~^]-?\\d*(\\.\\d+)?' },
      ],
    };
  });
  document.querySelectorAll('code.language-slab').forEach(function (block) {
    hljs.highlightBlock(block);
  });
})();
