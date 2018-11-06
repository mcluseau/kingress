package main

import (
	"io/ioutil"
	"log"
	"os"
	"os/exec"
	"text/template"

	"github.com/mcluseau/kingress/config"
)

var funcMap template.FuncMap = map[string]interface{}{
	"sh": func(script string) string {
		cmd := exec.Command("bash", "-c", script)
		bytes, err := cmd.CombinedOutput()

		if err != nil {
			log.Printf("sh: %q: %v", script, err)
		}

		return "$ " + script + "\n" + string(bytes)
	},
}

func main() {
	tmplBytes, err := ioutil.ReadAll(os.Stdin)
	if err != nil {
		log.Fatal(err)
	}

	tmpl := template.Must(template.New("").
		Funcs(funcMap).
		Parse(string(tmplBytes)))

	tmpl.Execute(os.Stdout, map[string]interface{}{
		"annotations": config.Annotations,
	})
}
