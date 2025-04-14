package main

import (
	"io"
	"log"
	"os"
	"os/exec"
	"text/template"
)

var funcMap template.FuncMap = map[string]any{
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
	tmplBytes, err := io.ReadAll(os.Stdin)
	if err != nil {
		log.Fatal(err)
	}

	tmpl := template.Must(template.New("").
		Funcs(funcMap).
		Parse(string(tmplBytes)))

	tmpl.Execute(os.Stdout, map[string]any{})
}
