-- Classification tags on knowledge documents. For RCAs these carry process
-- metrics the team counts (not graph analysis): origin:regression vs
-- origin:dormant, detectable-by:code-review / qa / automated-tests, and any
-- ad-hoc counting tags. Convention: "namespace:value" strings.
ALTER TABLE kb_documents
    ADD COLUMN tags TEXT[] NOT NULL DEFAULT '{}';
