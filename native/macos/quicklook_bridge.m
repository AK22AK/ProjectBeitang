#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <QuickLookUI/QuickLookUI.h>
#import <dispatch/dispatch.h>
#import <pthread.h>
#import <stdbool.h>
#import <stdint.h>
#import <stdio.h>
#import <string.h>

@interface BTQuickLookController : NSObject <QLPreviewPanelDataSource>
@property(nonatomic, strong, nullable) NSURL *previewURL;
+ (instancetype)sharedController;
- (BOOL)showPreviewForPath:(NSString *)path error:(NSError **)error;
@end

@implementation BTQuickLookController

+ (instancetype)sharedController {
    static BTQuickLookController *controller = nil;
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        controller = [[BTQuickLookController alloc] init];
    });
    return controller;
}

- (NSInteger)numberOfPreviewItemsInPreviewPanel:(QLPreviewPanel *)panel {
    return self.previewURL == nil ? 0 : 1;
}

- (id<QLPreviewItem>)previewPanel:(QLPreviewPanel *)panel previewItemAtIndex:(NSInteger)index {
    if (index != 0 || self.previewURL == nil) {
        return nil;
    }
    return self.previewURL;
}

- (BOOL)showPreviewForPath:(NSString *)path error:(NSError **)error {
    if (path.length == 0) {
        if (error != NULL) {
            *error = [NSError errorWithDomain:@"BTQuickLook"
                                         code:1
                                     userInfo:@{NSLocalizedDescriptionKey : @"预览路径为空"}];
        }
        return NO;
    }

    NSString *resolvedPath = [path stringByResolvingSymlinksInPath];
    if (![[NSFileManager defaultManager] fileExistsAtPath:resolvedPath]) {
        if (error != NULL) {
            *error = [NSError errorWithDomain:@"BTQuickLook"
                                         code:2
                                     userInfo:@{
                                         NSLocalizedDescriptionKey :
                                             [NSString stringWithFormat:@"预览文件不存在：%@", resolvedPath]
                                     }];
        }
        return NO;
    }

    self.previewURL = [NSURL fileURLWithPath:resolvedPath];
    if (self.previewURL == nil) {
        if (error != NULL) {
            *error = [NSError errorWithDomain:@"BTQuickLook"
                                         code:3
                                     userInfo:@{NSLocalizedDescriptionKey : @"无法创建预览文件 URL"}];
        }
        return NO;
    }

    [NSApp activateIgnoringOtherApps:YES];
    QLPreviewPanel *panel = [QLPreviewPanel sharedPreviewPanel];
    panel.dataSource = self;
    [panel reloadData];
    [panel setCurrentPreviewItemIndex:0];
    [panel makeKeyAndOrderFront:nil];
    [panel orderFrontRegardless];
    return YES;
}

@end

static void bt_write_error(char *buffer, uintptr_t buffer_len, NSString *message) {
    if (buffer == NULL || buffer_len == 0) {
        return;
    }

    const char *utf8 = message.UTF8String;
    if (utf8 == NULL) {
        buffer[0] = '\0';
        return;
    }

    snprintf(buffer, (size_t)buffer_len, "%s", utf8);
}

bool bt_quicklook_preview_file(const char *path, char *error_buffer, uintptr_t error_buffer_len) {
    @autoreleasepool {
        if (path == NULL) {
            bt_write_error(error_buffer, error_buffer_len, @"预览路径为空");
            return false;
        }

        __block BOOL success = NO;
        __block NSError *previewError = nil;
        NSString *nsPath = [NSString stringWithUTF8String:path];

        void (^showBlock)(void) = ^{
            success = [[BTQuickLookController sharedController] showPreviewForPath:nsPath
                                                                             error:&previewError];
        };

        if (pthread_main_np() != 0) {
            showBlock();
        } else {
            dispatch_sync(dispatch_get_main_queue(), showBlock);
        }

        if (!success) {
            NSString *message = previewError.localizedDescription ?: @"系统预览打开失败";
            bt_write_error(error_buffer, error_buffer_len, message);
            return false;
        }

        bt_write_error(error_buffer, error_buffer_len, @"");
        return true;
    }
}
