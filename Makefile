all: zhtta

zhtta: 
	rustc zhtta.rs

clean:
	rm -rf *~ zhtta
