```py
from rimera import cli

app = cli.App()

@app.command()
def build(release: bool = False, target: str = "native"):
    print(target)

app.run()
```

Since rimera already creates an executable, Let's implement Rimera more with a very clean CLI interface.