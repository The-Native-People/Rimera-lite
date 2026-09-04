from rimera import headlessbrowser

async def main():
    async with await headlessbrowser.launch() as browser:
        page = await browser.new_page()

        await page.goto("https://example.com" )
        print(await page.title())

        await page.locator("input[name=q]").fill("Rimera")
        await page.locator("button[type=submit]").click()

        print(await page.locator("body").text_content())
