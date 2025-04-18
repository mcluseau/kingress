
README.md: README.md.in utils/gendoc.go kingress
	go run utils/gendoc.go <$< >$@

