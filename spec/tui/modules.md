## Module-build loader

Current Rimera builds one entry module. Package/module graph discovery is not
implemented yet and must not be represented as active CLI output.

When package resolution lands, it will add explicit resolver stages to the
native build loader rather than framework-specific progress text.
