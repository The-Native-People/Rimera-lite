from rimera import server,err,json

app = server()

@app.get("/")
def home:
    return {
        "status": "200"
    }
# or use rimera short codes

@app.get("/error")
 def error():
     return err(
         500,
         message="Try again or report"
     )
     # it will then output
     # {
     #  "status": 500,
     #  "error": "Application didn't respond sucessfully"
     #  "message": "Try again or port"
     #  
     # }

app.get("/json")
def json():
    return json(dict)