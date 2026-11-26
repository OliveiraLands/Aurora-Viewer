#import <Foundation/Foundation.h>

extern int aurora_ios_main(void);

int main(int argc, char *argv[]) {
    @autoreleasepool {
        (void)argc;
        (void)argv;
        return aurora_ios_main();
    }
}
